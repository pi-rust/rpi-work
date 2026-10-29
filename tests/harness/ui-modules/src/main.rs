#![allow(dead_code, unused_imports)]
use dioxus::prelude::*;
#[path = "../../../../apps/work-ui/src/app/mod.rs"]
mod app;
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
    app::connections::use_connections("http://127.0.0.1:3030/");
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
        style { {include_str!("../../../../apps/work-ui/src/shared/theme/themes.css")} }
        style { "{resource_styles}" }
        style { {"html,body { overflow:auto; } .module-nav {display:flex;padding:10px;gap:15px;flex-wrap:wrap} .module-notice,.module-quote {padding:10px;overflow-wrap:anywhere;white-space:pre-wrap;max-width:100%}"} }
        nav { class: "module-nav",
            for name in ["meta", "packages", "skills", "extensions", "traces", "projects", "extras", "conversation", "settings", "queue", "scroll"] {
                button { onclick:move |_|page.set(name.into()), "{name}" }
            }
        }
        if page.read().as_str()=="scroll" {
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
        let mut client = use_signal(|| None::<work_client::WorkspaceClient>);
        let mut url = use_signal(String::new);
        let mut selected = use_signal(|| "dark".to_owned());
        let mut snapshot = use_signal(|| None::<work_client::QueueSnapshot>);
        let drafts = message_queue::use_queue_drafts();
        let native_agents =
            use_signal(std::collections::HashMap::<String, work_client::AgentClient>::new);
        let mut open = use_signal(|| false);
        let scope = format!("review-node/{}", selected.read());
        let session_id = selected.read().clone();
        return rsx! {
            style { {message_queue::CSS} }
            div {class:"module-nav",
                input {aria_label:"Fixture node URL",value:"{url}",oninput:move |e|url.set(e.value())}
                button {onclick:move |_|client.set(work_client::WorkspaceClient::new(&url.read()).ok()),"Connect fixture"}
                button {onclick:move |_|{selected.set("dark".into());snapshot.set(None);},"Session A"}
                button {onclick:move |_|{selected.set("architecture".into());snapshot.set(None);},"Session B"}
                message_queue::QueueEntry {snapshot:snapshot.read().clone(),onopen:move |_|open.set(true)}
            }
            div {class:"review-queue-shelf",message_queue::QueueShelf {snapshot:snapshot.read().clone(),onopen:move |_|open.set(true)}}
            if *open.read(){for (scope,session_id) in [(scope,session_id)]{
                message_queue::QueueDialog {key:"{scope}",scope,session_id,client,native_agents,drafts,snapshot:snapshot.read().clone(),onchange:move |value|snapshot.set(Some(value)),onclose:move |_|open.set(false)}
            }}
        };
    }
    #[cfg(not(feature = "queue"))]
    rsx! {}
}

#[component]
fn SettingsContent() -> Element {
    #[cfg(feature = "settings")]
    {
        let appearance = appearance::use_appearance();
        let mut appearance_open = use_signal(|| false);
        let mut client = use_signal(|| None::<work_client::WorkspaceClient>);
        let requests = use_signal(Vec::<dioxus::core::Task>::new);
        let active_page = use_signal(|| "settings".to_owned());
        let section = use_signal(|| "settings".to_owned());
        let url = use_signal(|| "http://127.0.0.1:3030/".to_owned());
        let mut status = use_signal(|| "未连接".to_owned());
        let mut auth = use_signal(|| None::<work_client::AuthState>);
        let password = use_signal(String::new);
        let runtime = use_signal(|| None::<work_client::RuntimeState>);
        let mut caps = use_signal(|| None::<work_client::Capabilities>);
        let selected_session = use_signal(|| Some("dark".to_owned()));
        let session_generation = use_signal(|| 0u64);
        return rsx! {
            settings::SettingsPage {shared_client:client,requests,active_page,settings_section:section,base_url:url,status,auth_state:auth,password,node_token:use_signal(String::new),runtime_state:runtime,capabilities:caps,selected_session,session_generation,
                onconnect:move |_| {
                    let endpoint=url.read().clone();
                    spawn(async move {
                        match work_client::WorkspaceClient::new(&endpoint) {
                            Ok(node)=> {
                                match node.health().await {Ok(_)=>status.set("节点在线".into()),Err(error)=>status.set(error.to_string())}
                                if let Ok(value)=node.auth().await {auth.set(Some(value));}
                                if let Ok(value)=node.capabilities().await {caps.set(Some(value));}
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
        let native_agents =
            use_signal(std::collections::HashMap::<String, work_client::AgentClient>::new);
        let shared_client = use_signal(|| None::<work_client::WorkspaceClient>);
        let session_generation = use_signal(|| 0u64);
        let requests = use_signal(Vec::<dioxus::core::Task>::new);
        let runtime_state = use_signal(|| None::<work_client::RuntimeState>);
        let status = use_signal(String::new);
        let mut quoted = use_signal(String::new);
        let open_requested = use_signal(|| false);
        let open_session_config = use_signal(|| false);
        return rsx! {
            div { class:"module-nav",
                button { onclick:move |_|session.set("review-session-a".into()), "Session A" }
                button { onclick:move |_|session.set("review-session-b".into()), "Session B" }
                resources::SessionResources {session_id:session.read().clone(), library, selected_session,shared_client,native_agents,session_generation,requests,runtime_state,status,open_requested,onquote:move |text|quoted.set(text)}
                resources::SkillPicker {session_id:session.read().clone(), library, open_session_config, onquote:move |text|quoted.set(text)}
            }
            resources::ResourcePage { kind, library }
            div { class:"module-quote", "{quoted}" }
        };
    }
    #[cfg(not(feature = "resources"))]
    rsx! {}
}
