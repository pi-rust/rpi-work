#![allow(dead_code, unused_imports)]
use dioxus::prelude::*;
#[path = "../../../../apps/work-ui/src/app/mod.rs"]
mod app;
mod extension_demo;
#[path = "../../../../apps/work-ui/src/features/mod.rs"]
mod features;
#[path = "../../../../apps/work-ui/src/shared/mod.rs"]
mod shared;
use features::{
    conversation::{composer_extras, message_queue, scroll_follow, timeline as conversation},
    meta_agent,
    observability::traces,
    projects::{project_files, project_picker},
    resources::{library as resources, session_config},
    settings,
};
use shared::{components, state, theme::appearance};
fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    app::connections::use_connections("http://127.0.0.1:13030/");
    features::settings::updates::use_updates();
    #[cfg(feature = "resources")]
    {
        let library = resources::use_resource_library();
        use_context_provider(|| library);
    }
    let mut page = use_signal(|| "meta".to_owned());
    let notice = use_signal(String::new);
    #[cfg(feature = "resources")]
    let resource_styles = resources::CSS;
    #[cfg(not(feature = "resources"))]
    let resource_styles = "";
    use_effect(|| {
        spawn(async {
            let _ = document::eval("document.documentElement.dataset.theme='paper';document.documentElement.dataset.mode='light';dioxus.send(true)").recv::<bool>().await;
        });
    });
    rsx! {
        style { {include_str!("../../../../apps/work-ui/src/shared/theme/styles.css")} }
        style { {appearance::STYLESHEETS} }
        style { "{resource_styles}" }
        style { {"html,body { overflow:auto; } .module-nav {display:flex;padding:10px;gap:15px;flex-wrap:wrap} .module-notice,.module-quote {padding:10px;overflow-wrap:anywhere;white-space:pre-wrap;max-width:100%}"} }
        nav { class: "module-nav",
            for name in ["meta", "packages", "skills", "extensions", "extension-demo", "traces", "projects", "extras", "conversation", "settings", "queue", "scroll"] {
                button { onclick:move |_|page.set(name.into()), "{name}" }
            }
        }
        if page.read().as_str()=="extension-demo" {
            extension_demo::ExtensionDemo {}
        } else if page.read().as_str()=="scroll" {
            ScrollContent {}
        } else if page.read().as_str()=="queue" {
            QueueContent {}
        } else if page.read().as_str()=="settings" {
            SettingsContent {}
        } else if page.read().as_str()=="conversation" {
            ConversationContent {}
        } else if page.read().as_str()=="extras" {
            ExtraContent {}
        } else if page.read().as_str()=="traces" {
            TraceContent {}
        } else if page.read().as_str()=="projects" {
            ProjectContent {}
        } else if page.read().as_str()=="meta" {
            MetaContent { notice }
        } else {
            ResourceContent { kind: page.read().clone() }
        }
        div { class: "module-notice", "{notice}" }
    }
}

#[component]
fn ScrollContent() -> Element {
    #[cfg(feature = "scroll")]
    {
        let mut session = use_signal(|| "session-a".to_owned());
        let mut loaded = use_signal(|| true);
        let mut visible = use_signal(|| true);
        let mut extra = use_signal(|| 0usize);
        let mut delta = use_signal(String::new);
        let mut generation = use_signal(|| 0u64);
        let mut switch = move |name: String| {
            generation += 1;
            let guard = *generation.read();
            session.set(name);
            loaded.set(false);
            spawn(async move {
                let _ = document::eval("await new Promise(r=>setTimeout(r,100));dioxus.send(true)")
                    .recv::<bool>()
                    .await;
                if *generation.read() == guard {
                    loaded.set(true);
                }
            });
        };
        let scope = format!("review-node/{}", session.read());
        return rsx! {
            style { {scroll_follow::CSS} }
            div {class:"module-nav",
                button {onclick:move |_|switch("session-a".into()),"Session A"}
                button {onclick:move |_|switch("session-b".into()),"Session B"}
                button {onclick:move |_|extra+=1,"Append progress"}
                button {onclick:move |_|{let text=format!("{}\\nStreamed public line",delta.read());delta.set(text)},"Stream last text"}
                button {onclick:move |_|{let show=!*visible.read();visible.set(show)},"Toggle conversation"}
            }
            if *visible.read(){
                div {class:"review-scroll-container transcript",style:"height:320px;flex:none;overflow:auto;border:1px solid var(--line);margin:20px;scroll-behavior:auto", "data-session-scope":"{scope}",
                    if *loaded.read(){
                        for index in 0..(60+*extra.read()) { article {key:"{scope}-{index}","data-message-id":"{scope}-{index}",style:"min-height:54px;padding:10px;border-bottom:1px solid var(--line)","{scope} · Message {index}"} }
                        p {style:"white-space:pre-wrap","{delta}"}
                    }
                }
                scroll_follow::ScrollFollow {scope:scope.clone(),selector:".review-scroll-container"}
            }
        };
    }
    #[cfg(not(feature = "scroll"))]
    rsx! {}
}

#[component]
fn QueueContent() -> Element {
    #[cfg(feature = "queue")]
    {
        use message_queue::{QueueAction, QueueStatus};
        let mut client = use_signal(|| None::<work_client::WorkspaceClient>);
        let mut url = use_signal(String::new);
        let mut connected_url = use_signal(String::new);
        let mut selected = use_signal(|| "dark".to_owned());
        let mut snapshot = use_signal(|| None::<work_client::QueueSnapshot>);
        let mut history = use_signal(|| None::<work_client::QueueSnapshot>);
        let mut session_access = use_signal(|| None::<work_client::SessionAccess>);
        let mut access_epoch = use_signal(|| 0u64);
        let mut busy = use_signal(|| false);
        let mut error = use_signal(|| None::<String>);
        // These are main drafts, not queue-specific editors. A late confirmed
        // withdrawal belongs to its original node/session, even after navigation.
        let mut drafts = use_signal(std::collections::HashMap::<(String, String), String>::new);
        let mut invalidate = move || {
            let next = access_epoch.peek().wrapping_add(1);
            access_epoch.set(next);
            session_access.set(None);
            snapshot.set(None);
            history.set(None);
            error.set(None);
            busy.set(true);
        };
        use_effect(move || {
            let epoch = *access_epoch.read();
            let node = client.peek().clone();
            let id = selected.peek().clone();
            let Some(node) = node else { busy.set(false); return; };
            spawn(async move {
                let access = node.session_access(&id).await;
                if *access_epoch.peek() != epoch || *selected.peek() != id { return; }
                match access {
                    Ok(access) => {
                        let supported = access.supports_command("get_queue");
                        session_access.set(Some(access));
                        if supported {
                            let result = node.queue(&id, false).await;
                            if *access_epoch.peek() != epoch || *selected.peek() != id { return; }
                            match result {
                                Ok(value) => snapshot.set(Some(value)),
                                Err(failure) => error.set(Some(failure.to_string())),
                            }
                        } else {
                            error.set(Some("此会话不支持读取队列".into()));
                        }
                    }
                    Err(failure) => error.set(Some(failure.to_string())),
                }
                busy.set(false);
            });
        });
        let mut act = move |action: QueueAction| {
            let (Some(node), Some(access)) = (client.peek().clone(), session_access.peek().clone()) else { return; };
            if *busy.peek() || !access.supports_command(action.command())
                || matches!(action, QueueAction::SendNow(_)) { return; }
            if let Some(item) = action.item() {
                if !snapshot.peek().as_ref().is_some_and(|value| value.items.iter().any(|current|
                    current == item && QueueStatus::from_wire(current.status.as_deref()).can_cancel())) { return; }
            }
            let epoch = *access_epoch.peek();
            let id = selected.peek().clone();
            let scope = (connected_url.peek().clone(), id.clone());
            busy.set(true);
            error.set(None);
            spawn(async move {
                let result = async {
                    match &action {
                        QueueAction::Refresh | QueueAction::History => {}
                        QueueAction::Clear => { node.clear_queue(&id).await.map_err(|e| e.to_string())?; }
                        QueueAction::Remove(item) | QueueAction::Edit(item) => {
                            let result = node.cancel_queued(&id, &item.entry_id).await.map_err(|e| e.to_string())?;
                            if result.entry_id != item.entry_id || !message_queue::cancellation_confirmed(&result.outcome) {
                                return Err(format!("消息未能撤回（{}），请刷新队列。", result.outcome));
                            }
                            if matches!(action, QueueAction::Edit(_)) {
                                let existing = drafts.peek().get(&scope).cloned().unwrap_or_default();
                                drafts.write().insert(scope.clone(), message_queue::recovered_text(&existing, &item.text));
                            }
                        }
                        QueueAction::SendNow(_) => unreachable!(),
                    }
                    Ok::<(), String>(())
                }.await;
                // Never let an old request overwrite the selected node/session,
                // including switching A -> B -> A or reconnecting the same URL.
                if *access_epoch.peek() != epoch || *selected.peek() != id { return; }
                let readback = node.queue(&id, matches!(action, QueueAction::History)).await;
                if *access_epoch.peek() != epoch || *selected.peek() != id { return; }
                match readback {
                    Ok(value) => {
                        if matches!(action, QueueAction::History) { history.set(Some(value.clone())); }
                        else { history.set(None); }
                        snapshot.set(Some(value));
                    }
                    Err(failure) => error.set(Some(failure.to_string())),
                }
                if let Err(failure) = result { error.set(Some(failure)); }
                busy.set(false);
            });
        };
        let scope = (connected_url.read().clone(), selected.read().clone());
        let draft = drafts.read().get(&scope).cloned().unwrap_or_default();
        let panel_key = format!("{scope:?}/{}", access_epoch.read());
        let can_cancel = session_access.read().as_ref().is_some_and(|access| access.supports_command("cancel_queued"));
        let can_clear = session_access.read().as_ref().is_some_and(|access| access.supports_command("clear_queue"));
        return rsx! {
            style { {message_queue::CSS} }
            div {class:"module-nav",
                input {aria_label:"Fixture node URL",value:"{url}",oninput:move |e|url.set(e.value())}
                button {onclick:move |_|{
                    connected_url.set(app::identity::node_origin(&url.peek()));
                    client.set(work_client::WorkspaceClient::new(&url.peek()).ok());
                    invalidate();
                },"Connect fixture"}
                button {onclick:move |_|{selected.set("dark".into());invalidate();},"Session A"}
                button {onclick:move |_|{selected.set("architecture".into());invalidate();},"Session B"}
            }
            div {class:"composer-main review-queue",style:"padding:12px;min-width:0;max-width:850px;margin:auto", "data-busy":"{busy}",
                message_queue::QueuePanel {
                    key:"{panel_key}", snapshot:snapshot.read().clone(), history:history.read().clone(),
                    busy:*busy.read(), error:error.read().clone(), can_cancel, can_clear, can_send:false,
                    onaction:move |action|act(action)
                }
                textarea {aria_label:"Main draft",value:"{draft}",style:"box-sizing:border-box;width:100%;min-width:0;min-height:100px",oninput:move |e|{drafts.write().insert(scope.clone(),e.value());}}
            }
        };
    }
    #[cfg(not(feature = "queue"))]
    rsx! {}
}

#[component]
fn SettingsContent() -> Element {
    #[cfg(feature = "settings")]
    {
        settings::use_pairing();
        let appearance = appearance::use_appearance();
        let mut appearance_open = use_signal(|| false);
        let mut client = use_signal(|| None::<work_client::WorkspaceClient>);
        let requests = use_signal(Vec::<dioxus::core::Task>::new);
        let active_page = use_signal(|| "settings".to_owned());
        let section = use_signal(|| "settings".to_owned());
        let url = use_signal(|| "http://127.0.0.1:13030/".to_owned());
        let mut status = use_signal(|| "未连接".to_owned());
        let mut auth = use_signal(|| None::<work_client::AuthState>);
        let password = use_signal(crate::app::connections::NodePassword::default);
        let runtime = use_signal(|| None::<work_client::RuntimeState>);
        let mut caps = use_signal(|| None::<work_client::Capabilities>);
        let selected_session = use_signal(|| Some("dark".to_owned()));
        let session_generation = use_signal(|| 0u64);
        let mut connection_epoch = use_signal(|| 0u64);
        let mut online = use_signal(|| false);
        return rsx! {
            settings::DesktopPairing { shared_client: client, base_url: url, auth_state: auth, view: section.read().clone() }
            settings::SettingsPage {shared_client:client,requests,active_page,settings_section:section,base_url:url,status,auth_state:auth,password,runtime_state:runtime,capabilities:caps,selected_session,session_generation,connection_epoch,online:online(),
                onconnect:move |_| {
                    let next=connection_epoch.peek().wrapping_add(1);
                    connection_epoch.set(next);
                    online.set(false);
                    client.set(None);
                    let endpoint=url.read().clone();
                    spawn(async move {
                        match work_client::WorkspaceClient::new(&endpoint) {
                            Ok(node)=> {
                                match node.health().await {Ok(_)=>status.set("节点在线".into()),Err(error)=>status.set(error.to_string())}
                                if let Ok(value)=node.auth().await {auth.set(Some(value));}
                                let capabilities=node.capabilities().await;
                                if *connection_epoch.peek()!=next {return;}
                                if let Ok(value)=capabilities {caps.set(Some(value));online.set(true);}
                                client.set(Some(node));
                            },
                            Err(error)=>status.set(error.to_string()),
                        }
                    });
                },
                onlogin:move |_|{},update_busy:false, mobile_client:false, onappearance:move |_|appearance_open.set(true)
            }
            if *appearance_open.read() {appearance::AppearancePicker {settings:appearance,onclose:move |_|appearance_open.set(false)}}
        };
    }
    #[cfg(not(feature = "settings"))]
    rsx! {}
}

#[component]
fn ExtraContent() -> Element {
    #[cfg(feature = "extras")]
    {
        let store = composer_extras::use_attachment_store();
        let btw_store = composer_extras::use_btw_store();
        let mut client = use_signal(|| None::<work_client::WorkspaceClient>);
        let mut url = use_signal(String::new);
        let mut scope = use_signal(|| "review-node/session-a".to_owned());
        let mut open = use_signal(|| false);
        let mut quote = use_signal(String::new);
        return rsx! {
            style { {composer_extras::CSS} }
            div { class:"module-nav",
                input { aria_label:"Fixture node URL",value:"{url}",oninput:move |e|url.set(e.value()) }
                button { onclick:move |_|client.set(work_client::WorkspaceClient::new(&url.read()).ok()),"Connect fixture" }
                button { onclick:move |_|scope.set("review-node/session-a".into()),"Session A" }
                button { onclick:move |_|scope.set("review-node/session-b".into()),"Session B" }
                button { onclick:move |_|open.set(true),"Open BTW" }
            }
            div { class:"composer-main",style:"padding:15px;max-width:850px",
                div {class:"review-attachment-files",composer_extras::AttachmentPicker {scope:scope.read().clone(),store,presentation:"files",onquote:move |text|quote.set(text)}}
                div {class:"review-attachment-button",composer_extras::AttachmentPicker {scope:scope.read().clone(),store,presentation:"button",onquote:move |text|quote.set(text)}}
            }
            if *open.read() {composer_extras::BtwPanel {scope:scope.read().clone(),project_id:Some("rpi-work".into()),client,store:btw_store,onquote:move |text|quote.set(text),onclose:move |_|open.set(false)}}
            pre { class:"module-quote","{quote}" }
        };
    }
    #[cfg(not(feature = "extras"))]
    rsx! {}
}

#[component]
fn ConversationContent() -> Element {
    #[cfg(feature = "conversation")]
    {
        let mut kind = use_signal(|| "single".to_owned());
        let mut answer = use_signal(String::new);
        let mut error = use_signal(|| None::<String>);
        let ui = match kind.read().as_str() {
            "multi" => {
                serde_json::json!({"purpose":"question","question":"选择验证范围","options":["桌面","手机","平板"],"allowMultiple":true})
            }
            "free" => {
                serde_json::json!({"purpose":"question","kind":"input","question":"说明改动原因"})
            }
            "permission" => {
                serde_json::json!({"purpose":"permission","kind":"confirm","question":"允许执行此命令？","options":[{"title":"允许一次","permission":"allow_once"},{"title":"拒绝","permission":"reject_once"}]})
            }
            _ => {
                serde_json::json!({"purpose":"question","kind":"selector","question":"选择技术栈","options":["Rust","TypeScript"],"allowFreeform":true})
            }
        };
        let pending:work_client::PendingDialog=serde_json::from_value(serde_json::json!({"requestId":format!("review-{}",kind.read()),"toolCallId":"tool-review","ui":ui})).expect("valid fixture dialog");
        let markdown = "# 公开回答\n\n**重点**与 `code`。\n\n- 项目一\n- 项目二\n\n> 一段引用\n\n```rust\nfn main() {}\n```\n\n[危险链接](javascript:alert(1))\n<script>window.__markdownExecuted=true</script>";
        return rsx! {
            style { {conversation::CSS} }
            div { class:"module-nav",
                for name in ["single","multi","free","permission"] { button {onclick:move |_|{kind.set(name.into());error.set(None);},"{name}"} }
                button {onclick:move |_|error.set(Some("节点拒绝了此次回答，选择仍保留。".into())),"Simulate response failure"}
            }
            div { style:"padding:20px;max-width:850px",class:"conversation-module-content",
                {conversation::markdown_view(markdown)}
                for pending in [pending] {conversation::AskCard {key:"{pending.request_id}",pending,submitting:false,error:error.read().clone(),onanswer:move |result:conversation::DialogAnswer|answer.set(serde_json::json!({"requestId":result.request_id,"answer":result.answer}).to_string())}}
                {conversation::render_message_block(&work_core::MessageBlock::ToolCall{id:"tool-review".into(),name:"read".into(),arguments:"{\"path\":\"src/main.rs\"}".into(),output:"public tool result".into(),status:work_core::ToolStatus::Completed})}
            }
            pre {class:"module-answers",style:"white-space:pre-wrap;overflow-wrap:anywhere","{answer}"}
        };
    }
    #[cfg(not(feature = "conversation"))]
    rsx! {}
}

#[component]
fn TraceContent() -> Element {
    #[cfg(feature = "trace")]
    {
        let store = traces::use_trace_store();
        let scope = "fixture-node/review-session".to_owned();
        let mut sequence = use_signal(|| 0u64);
        let mut emit = move |data: serde_json::Value| {
            sequence += 1;
            traces::observe_event(
                store,
                "fixture-node/review-session".into(),
                &work_client::WireEvent {
                    id: Some(sequence.read().to_string()),
                    seq: Some(*sequence.read()),
                    event_type: data["type"].as_str().unwrap_or("unknown").to_owned(),
                    data,
                },
            );
        };
        return rsx! {
            style { {traces::CSS} }
            div { class:"module-nav",
                button { onclick:move |_|emit(serde_json::json!({"type":"agent_start","runId":"review-live-run"})), "Start run" }
                button { onclick:move |_|emit(serde_json::json!({"type":"message_update","message":{"entryId":"answer-1","role":"assistant"},"eventType":"text_delta","delta":"Public streamed response"})), "First text" }
                button { onclick:move |_|emit(serde_json::json!({"type":"tool_execution_start","toolCallId":"review-read","toolName":"read","args":{"path":"src/main.rs","apiKey":"test-should-be-redacted"}})), "Start tool" }
                button { onclick:move |_|emit(serde_json::json!({"type":"tool_execution_end","toolCallId":"review-read","toolName":"read","result":"public file excerpt"})), "End tool" }
                button { onclick:move |_|emit(serde_json::json!({"type":"agent_end"})), "Agent end" }
                button { onclick:move |_|emit(serde_json::json!({"type":"agent_settled","runId":"review-live-run","outcome":"completed"})), "Settle run" }
            }
            traces::TraceEntry { scope,store }
        };
    }
    #[cfg(not(feature = "trace"))]
    rsx! {}
}

#[component]
fn ProjectContent() -> Element {
    #[cfg(feature = "project")]
    {
        let mut shared_client = use_signal(|| None::<work_client::WorkspaceClient>);
        let mut url = use_signal(String::new);
        let mut picker = use_signal(|| false);
        let mut search = use_signal(|| false);
        let mut notice = use_signal(String::new);
        let projects = vec![
            work_client::Project {
                id: "rpi-work".into(),
                name: "自定义工作台".into(),
                path: "D:/Projects/rpi-work".into(),
            },
            work_client::Project {
                id: "pi-rust".into(),
                name: "pi-rust".into(),
                path: "D:/Projects/pi-rust".into(),
            },
        ];
        let sessions = vec![work_client::SessionSummary {
            session_id: "draft-a".into(),
            project_id: Some("rpi-work".into()),
            title: Some("独立任务A".into()),
            status: "idle".into(),
            created_at: 0,
            updated_at: 0,
            last_seq: 0,
            error: None,
            pending_resume: false,
            archived: false,
            pinned: false,
            latest_settlement: None,
        }];
        return rsx! {
            style { {project_picker::CSS} }
            div { class:"module-nav",
                input { aria_label:"Fixture node URL",value:"{url}",oninput:move |e|url.set(e.value()) }
                button { onclick:move |_|{shared_client.set(work_client::WorkspaceClient::new(&url.read()).ok());},"Connect fixture" }
                button { onclick:move |_|picker.set(true),"Open project picker" }
                button { onclick:move |_|search.set(true),"Global search" }
            }
            if *picker.read(){project_picker::ProjectPicker{shared_client,projects:projects.clone(),busy:false,error:None,onsubmit:move |intent:project_picker::ProjectIntent|notice.set(serde_json::json!({"path":intent.path,"name":intent.name,"meta":intent.kind==project_picker::ProjectKind::MetaAgent,"create":intent.mode==project_picker::ProjectMode::Create,"initGit":intent.init_git}).to_string()),onconnectionrequired:move |_|{notice.set("请先连接节点".into());picker.set(false);},onclose:move |_|picker.set(false)}}
            if *search.read(){project_picker::SearchDialog{projects,sessions,onproject:move |id|notice.set(id),onsession:move |id|notice.set(id),onclose:move |_|search.set(false)}}
            pre { class:"module-notice","{notice}" }
        };
    }
    #[cfg(not(feature = "project"))]
    rsx! {}
}

#[component]
fn MetaContent(mut notice: Signal<String>) -> Element {
    #[cfg(feature = "meta")]
    {
        let store = meta_agent::use_meta_store();
        #[cfg(feature = "resources")]
        let library = use_context::<Signal<resources::ResourceLibrary>>();
        return rsx! { meta_agent::MetaAgentPage {
            project_id:"review-meta".to_owned(), project_name:"审查专用元Agent".to_owned(), store,
            ondevelop:move |resource:meta_agent::MetaResource|notice.set(format!("{}@{}#{}",resource.id,resource.version,resource.revision)),
            onpublish:move |value: serde_json::Value| {
                #[cfg(feature = "resources")]
                { let mut library=library; let result=resources::import_design_package(&mut library.write(),&value); notice.set(format!("{result:?}")); result }
                #[cfg(not(feature = "resources"))]
                { notice.set(value.to_string()); Ok::<String,String>("review-published".into()) }
            }
        } };
    }
    #[cfg(not(feature = "meta"))]
    rsx! {}
}

#[component]
fn ResourceContent(kind: String) -> Element {
    #[cfg(feature = "resources")]
    {
        let library = use_context::<Signal<resources::ResourceLibrary>>();
        let mut session = use_signal(|| "review-session-a".to_owned());
        let selected_session = use_signal(|| Some("review-session-a".to_owned()));
        let shared_client = use_signal(|| None::<work_client::WorkspaceClient>);
        let session_generation = use_signal(|| 0u64);
        let requests = use_signal(Vec::<dioxus::core::Task>::new);
        let runtime_state = use_signal(|| None::<work_client::RuntimeState>);
        let status = use_signal(String::new);
        let mut quoted = use_signal(String::new);
        let mut extension_locked = use_signal(|| false);
        let open_requested = use_signal(|| false);
        let open_session_config = use_signal(|| false);
        return rsx! {
            div { class:"module-nav",
                button { onclick:move |_|session.set("review-session-a".into()), "Session A" }
                button { onclick:move |_|session.set("review-session-b".into()), "Session B" }
                button { onclick:move |_| { let locked = *extension_locked.read(); extension_locked.set(!locked); }, "Toggle extension lock" }
                resources::SessionResources {session_id:session.read().clone(), extension_config_locked:*extension_locked.read(), library, selected_session,shared_client,session_generation,requests,runtime_state,status,open_requested,onquote:move |text|quoted.set(text)}
                resources::SkillPicker {session_id:session.read().clone(), library, open_session_config, onquote:move |text|quoted.set(text)}
            }
            resources::ResourcePage { kind, library }
            div { class:"module-quote", "{quoted}" }
        };
    }
    #[cfg(not(feature = "resources"))]
    rsx! {}
}
