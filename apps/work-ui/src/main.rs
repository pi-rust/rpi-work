use dioxus::core::Task;
use dioxus::prelude::*;
use runtime_stats::CompactResult;
use serde_json::{Value, json};
use work_client::{
    AuthState, Capabilities, Diff, FileListing, GitStatus, PendingDialog, Project, QueueSnapshot,
    RuntimeState, SessionSummary, WorkClient,
};
use work_core::{ConnectionState, MessageBlock, RunPhase, RunProjection, SettlementOutcome};

mod appearance;
mod components;
mod composer_extras;
mod conversation;
mod device_mode;
mod message_queue;
mod meta_agent;
mod project_files;
mod project_picker;
mod resources;
mod runtime_stats;
mod scroll_follow;
mod session_config;
mod state;
mod traces;
mod updates;

use appearance::{AppearancePicker, use_appearance};
mod settings;
use components::{Icon, Modal};
use settings::SettingsPage;
use state::{Drafts, update_draft};

const CSS: &str = include_str!("styles.css");
const THEMES: &str = include_str!("themes.css");
const RESOURCES_CSS: &str = resources::CSS;
const META_AGENT_CSS: &str = include_str!("meta_agent.css");
const PROJECT_PICKER_CSS: &str = project_picker::CSS;
const TRACES_CSS: &str = traces::CSS;
const CONVERSATION_CSS: &str = conversation::CSS;
const EXTRAS_CSS: &str = composer_extras::CSS;
const QUEUE_CSS: &str = message_queue::CSS;

fn main() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if std::env::args().nth(1).as_deref() == Some("--apply-update") {
        let result = std::env::args_os()
            .nth(2)
            .ok_or_else(|| "缺少更新计划".to_owned())
            .and_then(|path| work_update::native::run_helper(std::path::Path::new(&path)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let appearance = use_appearance();
    let device_mode = device_mode::use_device_mode();
    updates::use_updates();
    let initial_node_url = default_node_url();
    #[allow(unused_mut)] // Mutated only by the browser's same-origin initialization.
    let mut base_url = use_signal(|| initial_node_url.clone());
    let mut shared_client = use_signal(|| WorkClient::new(&initial_node_url).ok());
    let mut node_address_touched = use_signal(|| false);
    let mut search = use_signal(String::new);
    let mut change_query = use_signal(String::new);
    let mut change_status_filter = use_signal(|| "all".to_owned());
    let mut changes_unreviewed = use_signal(|| false);
    let mut reviewed_changes = use_signal(std::collections::HashSet::<String>::new);
    let mut global_search_open = use_signal(|| false);
    let mut picker_search = use_signal(String::new);
    let mut queue_open = use_signal(|| false);
    let mut status = use_signal(|| "未连接".to_owned());
    let mut auth_state = use_signal(|| None::<AuthState>);
    let mut password = use_signal(String::new);
    let mut capabilities = use_signal(|| None::<Capabilities>);
    let mut sessions = use_signal(Vec::<SessionSummary>::new);
    let mut projects = use_signal(Vec::<Project>::new);
    let mut selected_project = use_signal(|| None::<String>);
    let mut selected_project_name = use_signal(|| None::<String>);
    let mut selected_project_path = use_signal(|| None::<String>);
    // Project-file browsing and Git review deliberately have independent state.
    // Opening a file must never replace the review selection, patch, or scroll position.
    let mut files = use_signal(|| None::<FileListing>);
    let mut file_content = use_signal(|| None::<work_client::FileContent>);
    let mut git_status = use_signal(|| None::<GitStatus>);
    let mut selected_project_file = use_signal(|| None::<String>);
    let mut file_generation = use_signal(|| 0u64);
    let mut selected_change_file = use_signal(|| None::<String>);
    let mut change_generation = use_signal(|| 0u64);
    let mut change_diff = use_signal(|| None::<Diff>);
    let mut change_error = use_signal(|| None::<String>);
    let mut selected_session = use_signal(|| None::<String>);
    let mut selected_session_title = use_signal(|| None::<String>);
    let mut session_generation = use_signal(|| 0u64);
    let requests = use_signal(Vec::<Task>::new);
    let submissions = use_signal(Vec::<Task>::new);
    let mut pending_open = use_signal(|| None::<(bool, String)>);
    let mut node_url = use_signal(|| initial_node_url.clone());
    let mut active_view = use_signal(|| "conversation".to_owned());
    let mut sidebar_open = use_signal(|| false);
    use_effect(move || {
        let open = *sidebar_open.read();
        spawn(async move {
            let script = format!(
                r"
                const side = document.querySelector('.sidebar');
                const menu = document.querySelector('.mobile-menu');
                const sync = () => {{ side.inert = matchMedia('(max-width:700px)').matches && !{open}; }};
                sync();
                if ({open}) side.querySelector('input,button,a')?.focus();
                else if (side.contains(document.activeElement)) menu?.focus();
                dioxus.send(true);
            "
            );
            let _ = document::eval(&script).recv::<bool>().await;
        });
    });
    let mut inspector_open = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            let mut viewport = document::eval(
                r"
                const overlay = matchMedia('(max-width:1100px)');
                const phone = matchMedia('(max-width:700px)');
                const sync = () => {
                    const side = document.querySelector('.sidebar');
                    side.inert = phone.matches && !side.classList.contains('open');
                    dioxus.send(overlay.matches);
                };
                overlay.addEventListener('change', sync);
                phone.addEventListener('change', sync);
                sync();
            ",
            );
            while let Ok(is_overlay) = viewport.recv::<bool>().await {
                inspector_open.set(!is_overlay);
            }
        });
    });
    let mut project_files_open = use_signal(|| false);
    let mut project_file_preview = use_signal(|| false);
    let mut reported_files = use_signal(|| (String::new(), Vec::<work_client::FileEntry>::new()));
    let mut project_file_mode = use_signal(|| "preview".to_owned());
    let mut project_file_wrap = use_signal(|| true);
    let mut file_loading = use_signal(|| false);
    let mut file_error = use_signal(|| None::<String>);
    let mut change_directory_filter = use_signal(String::new);
    let mut change_mobile_preview = use_signal(|| false);
    let mut session_switcher_open = use_signal(|| false);
    let mut new_session_open = use_signal(|| false);
    let mut new_session_busy = use_signal(|| false);
    let mut new_session_error = use_signal(|| None::<String>);
    let mut created_session_id = use_signal(|| None::<String>);
    let mut session_title_draft = use_signal(String::new);
    let mut transcript = use_signal(Vec::<Value>::new);
    let mut projection = use_signal(|| None::<RunProjection>);
    let drafts = use_signal(Drafts::new);
    let mut sending = use_signal(std::collections::HashSet::<String>::new);
    let mut pending_enter = use_signal(|| None::<(String, u64, u64)>);
    let mut queue = use_signal(|| None::<QueueSnapshot>);
    let queue_drafts = message_queue::use_queue_drafts();
    let attachment_store = composer_extras::use_attachment_store();
    let btw_store = composer_extras::use_btw_store();
    let mut runtime_state = use_signal(|| None::<RuntimeState>);
    let mut last_turn_usage = use_signal(|| None::<(String, Value)>);
    let mut context_open = use_signal(|| false);
    let mut stats_open = use_signal(|| false);
    let mut compact_revision = use_signal(|| 0u64);
    let mut compact_pending = use_signal(|| None::<u64>);
    let mut compact_operation_accepted = use_signal(|| false);
    let mut node_compacting = use_signal(|| false);
    let mut compact_result = use_signal(|| None::<CompactResult>);
    use_effect(move || {
        let _ = *session_generation.read();
        context_open.set(false);
        stats_open.set(false);
        compact_pending.set(None);
        compact_operation_accepted.set(false);
        node_compacting.set(false);
        compact_result.set(None);
        last_turn_usage.set(None);
    });
    let mut pending_dialogs = use_signal(Vec::<PendingDialog>::new);
    let mut dialog_busy = use_signal(std::collections::HashSet::<String>::new);
    let mut dialog_errors = use_signal(std::collections::HashMap::<String, String>::new);
    let mut answered_dialogs = use_signal(std::collections::HashSet::<String>::new);
    let mut btw_open = use_signal(|| false);
    let mut queue_delivery = use_signal(|| "followUp".to_owned());
    let mut delete_confirmation = use_signal(|| false);
    let mut show_project_form = use_signal(|| false);
    let mut project_busy = use_signal(|| false);
    let mut project_error = use_signal(|| None::<String>);
    let mut project_generation = use_signal(|| 0u64);
    let project_requests = use_signal(Vec::<dioxus::core::Task>::new);
    let mut project_name_open = use_signal(|| false);
    let mut editing_project_id = use_signal(|| None::<String>);
    let mut project_name_error = use_signal(|| None::<String>);
    let mut project_aliases = use_signal(std::collections::HashMap::<String, String>::new);
    let mut project_aliases_loaded = use_signal(|| false);
    let mut active_page = use_signal(|| "workspace".to_owned());
    let mut settings_section = use_signal(|| "settings".to_owned());
    let mut appearance_open = use_signal(|| false);
    let mut session_details_open = use_signal(|| false);
    // These stores live at App scope: every page and session receives the same binding.
    let mut resource_library = resources::use_resource_library();
    let mut meta_store = meta_agent::use_meta_store();
    let open_session_config = use_signal(|| false);
    let mut project_purposes = use_signal(std::collections::HashMap::<String, String>::new);
    let mut meta_return_page = use_signal(|| "workspace".to_owned());
    let development_associations = use_signal(std::collections::HashMap::<String, String>::new);
    let traces = traces::use_trace_store();
    let mut project_purposes_loaded = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        spawn(async move {
            let origin = document::eval("dioxus.send(window.location.origin + '/')")
                .recv::<String>()
                .await;
            if let Ok(origin) = origin {
                if !*node_address_touched.read() && base_url.read().as_str() == "about:blank" {
                    base_url.set(origin.clone());
                    node_url.set(origin.clone());
                    shared_client.set(WorkClient::new(&origin).ok());
                }
            }
        });
    });

    use_effect(move || {
        spawn(async move {
            if let Ok(value) = document::eval("try { dioxus.send(JSON.parse(localStorage.getItem('rpi-work-project-purposes-v1') || '{}')); } catch (_) { dioxus.send({}); }").recv::<Value>().await {
                if let Ok(items) = serde_json::from_value::<std::collections::HashMap<String, String>>(value) {
                    project_purposes.set(items);
                }
            }
            project_purposes_loaded.set(true);
        });
    });
    use_effect(move || {
        let value =
            serde_json::to_string(&*project_purposes.read()).unwrap_or_else(|_| "{}".into());
        if !*project_purposes_loaded.read() {
            return;
        }
        spawn(async move {
            let script = format!(
                "try {{ localStorage.setItem('rpi-work-project-purposes-v1', {}); }} catch (_) {{}} dioxus.send(true);",
                serde_json::to_string(&value).unwrap_or_default()
            );
            let _ = document::eval(&script).recv::<bool>().await;
        });
    });

    use_effect(move || {
        spawn(async move {
            if let Ok(value) = document::eval("try { dioxus.send(JSON.parse(localStorage.getItem('rpi-work-project-aliases-v1') || '{}')); } catch (_) { dioxus.send({}); }").recv::<Value>().await {
                if let Ok(items) = serde_json::from_value::<std::collections::HashMap<String, String>>(value) { project_aliases.set(items); }
            }
            project_aliases_loaded.set(true);
        });
    });
    use_effect(move || {
        if !*project_aliases_loaded.read() {
            return;
        }
        let value = serde_json::to_string(&*project_aliases.read()).unwrap_or_else(|_| "{}".into());
        spawn(async move {
            let script = format!(
                "try {{ localStorage.setItem('rpi-work-project-aliases-v1', {}); }} catch (_) {{}} dioxus.send(true);",
                serde_json::to_string(&value).unwrap_or_default()
            );
            let _ = document::eval(&script).recv::<bool>().await;
        });
    });

    let connect = move |_| {
        node_address_touched.set(true);
        let url = base_url.read().trim().to_owned();
        if *node_url.read() != url {
            state::cancel_requests(requests);
            state::cancel_requests(project_requests);
            project_generation += 1;
            project_busy.set(false);
            project_error.set(None);
            state::cancel_requests(submissions);
            session_generation += 1;
            file_generation += 1;
            change_generation += 1;
            shared_client.set(None);
            auth_state.set(None);
            capabilities.set(None);
            sessions.set(Vec::new());
            projects.set(Vec::new());
            project_name_open.set(false);
            editing_project_id.set(None);
            project_name_error.set(None);
            selected_project.set(None);
            selected_project_name.set(None);
            selected_project_path.set(None);
            selected_session.set(None);
            selected_session_title.set(None);
            files.set(None);
            file_content.set(None);
            git_status.set(None);
            selected_project_file.set(None);
            selected_change_file.set(None);
            change_diff.set(None);
            projection.set(None);
            transcript.set(Vec::new());
            runtime_state.set(None);
            queue.set(None);
            pending_dialogs.set(Vec::new());
            dialog_busy.write().clear();
            btw_open.set(false);
            queue_open.set(false);
            sending.set(std::collections::HashSet::new());
            password.set(String::new());
            node_url.set(url.clone());
        }
        if shared_client.read().is_none() {
            match WorkClient::new(&url) {
                Ok(value) => shared_client.set(Some(value)),
                Err(error) => {
                    status.set(format!("地址无效：{error}"));
                    return;
                }
            }
        }
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            status.set("连接中…".into());
            match client_result {
                Ok(client) => match client.health().await {
                    Ok(health) if health.ok => match client.auth().await {
                        Ok(auth) if auth.required && !auth.authenticated => {
                            auth_state.set(Some(auth));
                            status.set("节点需要密码认证".into());
                        }
                        Ok(auth) => {
                            auth_state.set(Some(auth));
                            match (client.capabilities().await, client.sessions().await) {
                                (Ok(caps), Ok(items)) => {
                                    let project_items = client.projects().await.unwrap_or_default();
                                    capabilities.set(Some(caps));
                                    sessions.set(items);
                                    projects.set(project_items);
                                    status.set("在线 · 可选择会话或项目".into());
                                }
                                (Err(error), _) | (_, Err(error)) => {
                                    status.set(format!("协议读取失败：{error}"));
                                }
                            }
                        }
                        Err(error) => status.set(format!("认证状态读取失败：{error}")),
                    },
                    Ok(_) => status.set("节点未就绪".into()),
                    Err(error) => status.set(connection_failure_message(&error.to_string())),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let login = move |_: MouseEvent| {
        let draft_url = base_url.read().trim().to_owned();
        if *node_url.read() != draft_url {
            status.set("请先连接当前地址，再登录".into());
            return;
        }
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let secret = password.read().clone();
        if secret.is_empty() {
            status.set("请输入节点密码".into());
            return;
        }
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.login(&secret).await {
                    Ok(auth) => {
                        auth_state.set(Some(auth));
                        password.set(String::new());
                        status.set("认证成功，请重新连接节点".into());
                    }
                    Err(error) => status.set(format!("登录失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };
    let submit_project = move |intent: project_picker::ProjectIntent| {
        if *project_busy.read() {
            return;
        }
        let Some((key, _)) = state::directory_key(&intent.path) else {
            project_error.set(Some("请输入有效的绝对目录路径。".into()));
            return;
        };
        let existing = projects
            .read()
            .iter()
            .find(|project| state::directory_key(&project.path).is_some_and(|(id, _)| id == key))
            .cloned();
        if let Some(project) = existing {
            show_project_form.set(false);
            pending_open.set(Some((true, project.id)));
            status.set("已复用同目录项目，未覆盖原有用途和名称".into());
            return;
        }
        let generation = *project_generation.read() + 1;
        project_generation.set(generation);
        project_busy.set(true);
        project_error.set(None);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let purpose = match intent.kind {
            project_picker::ProjectKind::Normal => "普通项目",
            project_picker::ProjectKind::MetaAgent => "元Agent",
        }
        .to_owned();
        let origin = node_origin(node_url.read().as_str());
        let target_path = intent.path.clone();
        let target_name = intent.name.clone();
        let mode = intent.mode;
        let init_git = intent.init_git;
        state::scoped_spawn(project_requests, async move {
            let result = async {
                let client = client_result.map_err(|error| error.to_string())?;
                let (server_name, server_path) = match mode {
                    project_picker::ProjectMode::Open => {
                        (target_name.clone(), Some(target_path.clone()))
                    }
                    project_picker::ProjectMode::Create => {
                        let parent = project_picker::directory_parent(&target_path)
                            .ok_or_else(|| "新建目录必须位于节点项目根目录下。".to_owned())?;
                        let folders = client
                            .project_folders(&parent)
                            .await
                            .map_err(|error| format!("无法读取节点项目根目录：{error}"))?;
                        let name = project_picker::direct_child_name(&folders.root, &target_path)
                            .ok_or_else(|| {
                            format!(
                                "当前节点只支持在项目根目录的直接子目录中新建：{}",
                                folders.root
                            )
                        })?;
                        (name, None)
                    }
                };
                client
                    .create_project(
                        &server_name,
                        server_path.as_deref(),
                        if mode == project_picker::ProjectMode::Create {
                            init_git
                        } else {
                            false
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            if *project_generation.read() != generation {
                return;
            }
            project_busy.set(false);
            match result {
                Ok(project) => {
                    let mut items = projects.read().to_vec();
                    items.push(project.clone());
                    projects.set(items);
                    let scope = project_scope_key(&origin, &project.id);
                    project_purposes.write().insert(scope.clone(), purpose);
                    if target_name != project.name {
                        project_aliases.write().insert(scope, target_name);
                    }
                    pending_open.set(Some((true, project.id.clone())));
                    show_project_form.set(false);
                    project_error.set(None);
                    status.set(format!("项目已添加：{}", project.name));
                }
                Err(error) => {
                    project_error.set(Some(format!("添加项目失败：{error}")));
                    status.set(format!("添加项目失败：{error}"));
                }
            }
        });
    };
    let submit_new_session = move |intent: project_picker::SessionIntent| {
        new_session_busy.set(true);
        new_session_error.set(None);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let project_id = intent.project_id.clone();
        let title = intent.title.clone();
        let existing_id = created_session_id.read().clone();
        state::scoped_spawn(requests, async move {
            let result = async {
                let client = client_result.map_err(|error| error.to_string())?;
                let session_id = if let Some(id) = existing_id {
                    id
                } else {
                    client
                        .create_session(project_id.as_deref())
                        .await
                        .map_err(|error| error.to_string())?
                };
                created_session_id.set(Some(session_id.clone()));
                if let Err(error) = client.rename_session(&session_id, &title).await {
                    return Err(format!(
                        "会话已创建，但命名失败：{error}（再次提交将继续使用此会话）"
                    ));
                }
                let items = client.sessions().await.map_err(|error| error.to_string())?;
                sessions.set(items);
                created_session_id.set(None);
                pending_open.set(Some((false, session_id)));
                Ok::<(), String>(())
            }
            .await;
            new_session_busy.set(false);
            match result {
                Ok(()) => {
                    new_session_open.set(false);
                    status.set("任务已创建".into());
                }
                Err(error) => {
                    new_session_error.set(Some(error.clone()));
                    status.set(error);
                }
            }
        });
    };

    let save_project_alias = move |name: String| {
        let Some(project_id) = editing_project_id.read().clone() else {
            return;
        };
        let scope = project_scope_key(&node_origin(node_url.read().as_str()), &project_id);
        project_aliases.write().insert(scope, name.clone());
        if selected_project.read().as_ref() == Some(&project_id) {
            selected_project_name.set(Some(name));
        }
        project_name_open.set(false);
        editing_project_id.set(None);
        project_name_error.set(None);
    };
    let rename_session = move |_: MouseEvent| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        let title = session_title_draft.read().trim().to_owned();
        if title.is_empty() {
            return;
        }
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.rename_session(&session_id, &title).await {
                    Ok(_) => {
                        session_title_draft.set(String::new());
                        selected_session_title.set(Some(title.clone()));
                        if let Ok(items) = client.sessions().await {
                            sessions.set(items);
                        }
                        status.set("会话已重命名".into());
                    }
                    Err(error) => status.set(format!("重命名失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };
    let fork_session = move |_: MouseEvent| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        let title = session_title_draft.read().trim().to_owned();
        let title = (!title.is_empty()).then_some(title);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client
                    .fork_session(&session_id, None, title.as_deref())
                    .await
                {
                    Ok(result) => {
                        if let Ok(items) = client.sessions().await {
                            sessions.set(items);
                        }
                        status.set(format!("已 fork 会话 {}", result.session_id));
                    }
                    Err(error) => status.set(format!("fork 失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let clone_session = move |_: MouseEvent| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        let title = session_title_draft.read().trim().to_owned();
        let title = (!title.is_empty()).then_some(title);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.clone_session(&session_id, title.as_deref()).await {
                    Ok(result) => {
                        if let Ok(items) = client.sessions().await {
                            sessions.set(items);
                        }
                        status.set(format!("已 clone 会话 {}", result.session_id));
                    }
                    Err(error) => status.set(format!("clone 失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let delete_session = move |_: MouseEvent| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.delete_session(&session_id).await {
                    Ok(()) => {
                        let still_selected =
                            selected_session.read().as_deref() == Some(session_id.as_str());
                        if still_selected {
                            let generation = *session_generation.read() + 1;
                            session_generation.set(generation);

                            selected_session.set(None);
                            selected_session_title.set(None);
                            delete_confirmation.set(false);
                            queue.set(None);
                            runtime_state.set(None);
                            pending_dialogs.set(Vec::new());
                            transcript.set(Vec::new());
                            projection.set(None);
                        }
                        if let Ok(items) = client.sessions().await {
                            sessions.set(items);
                        }
                        status.set("会话已删除".into());
                    }
                    Err(error) => status.set(format!("删除会话失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };
    let mut open_session = move |session_id: String| {
        state::cancel_requests(requests);
        transcript.set(Vec::new());
        file_generation += 1;
        pending_dialogs.set(Vec::new());
        dialog_busy.write().clear();
        btw_open.set(false);
        queue_open.set(false);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        sidebar_open.set(false);
        active_page.set("workspace".to_owned());
        let generation = *session_generation.read() + 1;
        session_generation.set(generation);
        let trace_scope = session_scope_key(&node_url.read(), &session_id);
        delete_confirmation.set(false);
        let project_id = sessions
            .read()
            .iter()
            .find(|session| session.session_id == session_id)
            .and_then(|session| session.project_id.clone());
        if let Some(session) = sessions
            .read()
            .iter()
            .find(|session| session.session_id == session_id)
        {
            selected_session_title.set(session.title.clone());
        } else {
            selected_session_title.set(None);
        }
        if project_id.is_none() {
            selected_project.set(None);
            selected_project_name.set(None);
            selected_project_path.set(None);
            files.set(None);
            file_content.set(None);
            git_status.set(None);
            selected_project_file.set(None);
            selected_change_file.set(None);
            change_diff.set(None);
            change_error.set(None);
        }
        if let Some(project_id) = project_id.as_ref() {
            selected_project.set(Some(project_id.clone()));
            if let Some(project) = projects
                .read()
                .iter()
                .find(|project| &project.id == project_id)
            {
                let scope = project_scope_key(&node_origin(node_url.read().as_str()), &project.id);
                let name = project_aliases
                    .read()
                    .get(&scope)
                    .cloned()
                    .unwrap_or_else(|| project.name.clone());
                selected_project_name.set(Some(name));
                selected_project_path.set(Some(project.path.clone()));
            }
            files.set(None);
            file_content.set(None);
            git_status.set(None);
            selected_project_file.set(None);
            selected_change_file.set(None);
            change_diff.set(None);
            change_error.set(None);
        }
        selected_session.set(Some(session_id.clone()));
        queue.set(None);
        runtime_state.set(None);
        projection.set(None);
        pending_dialogs.set(Vec::new());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.connect_session(session_id.clone()).await {
                    Ok(mut connection) => {
                        if *session_generation.read() != generation {
                            return;
                        }
                        if let Some(project_id) = project_id.as_deref() {
                            if let Ok(listing) = client.files(project_id, "").await {
                                files.set(Some(listing));
                            }
                            if let Ok(repository) = client.git_status(project_id).await {
                                git_status.set(Some(repository));
                            }
                        }
                        if let Ok(snapshot) = client.queue(&session_id, false).await {
                            queue.set(Some(snapshot));
                        }
                        if let Ok(state) = client.runtime_state(&session_id).await {
                            runtime_state.set(Some(state));
                        }
                        pending_dialogs.set(
                            connection
                                .pending_dialogs()
                                .iter()
                                .filter(|item| {
                                    !answered_dialogs
                                        .read()
                                        .contains(&format!("{trace_scope}::{}", item.request_id))
                                })
                                .cloned()
                                .collect(),
                        );
                        projection.set(Some(connection.projection().clone()));
                        traces::restore_history(
                            traces,
                            &trace_scope,
                            connection.projection().blocks.clone(),
                        );
                        transcript.set(connection.messages().to_vec());
                        status.set("会话在线 · SSE 已连接".into());
                        loop {
                            if *session_generation.read() != generation {
                                break;
                            }
                            match connection.next().await {
                                Ok(Some(event)) => {
                                    traces::observe_event(traces, &trace_scope, &event.wire);
                                    if *session_generation.read() != generation {
                                        break;
                                    }
                                    if event.wire.event_type == "connection_reconnecting" {
                                        status.set("SSE 连接中断，正在重试…".into());
                                    } else if event.wire.event_type == "connection_online" {
                                        status.set("SSE 已恢复，快照已重新校准".into());
                                        pending_dialogs.set(
                                            connection
                                                .pending_dialogs()
                                                .iter()
                                                .filter(|item| {
                                                    !answered_dialogs.read().contains(&format!(
                                                        "{trace_scope}::{}",
                                                        item.request_id
                                                    ))
                                                })
                                                .cloned()
                                                .collect(),
                                        );
                                    }
                                    if matches!(
                                        event.wire.event_type.as_str(),
                                        "ui_dialog_request" | "ui_dialog_closed"
                                    ) {
                                        if event.wire.event_type == "ui_dialog_closed" {
                                            if let Some(id) = event
                                                .wire
                                                .data
                                                .get("requestId")
                                                .and_then(Value::as_str)
                                            {
                                                let key = format!("{trace_scope}::{id}");
                                                answered_dialogs.write().remove(&key);
                                                dialog_errors.write().remove(&key);
                                            }
                                        }
                                        pending_dialogs.set(
                                            connection
                                                .pending_dialogs()
                                                .iter()
                                                .filter(|item| {
                                                    !answered_dialogs.read().contains(&format!(
                                                        "{trace_scope}::{}",
                                                        item.request_id
                                                    ))
                                                })
                                                .cloned()
                                                .collect(),
                                        );
                                    }
                                    projection.set(Some(connection.projection().clone()));
                                    transcript.set(connection.messages().to_vec());
                                    let event_type = event.wire.event_type.as_str();
                                    if matches!(
                                        event_type,
                                        "auto_compaction_start" | "compaction_start"
                                    ) {
                                        if !*node_compacting.peek() {
                                            compact_revision += 1;
                                        }
                                        node_compacting.set(true);
                                    }
                                    if event_type == "operation_accepted"
                                        && event.wire.data.get("command").and_then(Value::as_str)
                                            == Some("compact")
                                    {
                                        compact_operation_accepted.set(true);
                                        compact_pending.set(Some(*compact_revision.peek()));
                                        compact_result.set(None);
                                        status.set("节点已接受压缩请求，等待实际压缩结算…".into());
                                    }
                                    if event_type == "compaction_progress" {
                                        let manual =
                                            event.wire.data.get("manual").and_then(Value::as_bool)
                                                == Some(true);
                                        let active = event
                                            .wire
                                            .data
                                            .get("active")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false);
                                        if active {
                                            if !*node_compacting.peek() {
                                                compact_revision += 1;
                                            }
                                            node_compacting.set(true);
                                            if manual {
                                                compact_operation_accepted.set(true);
                                                if compact_pending.peek().is_none() {
                                                    compact_pending
                                                        .set(Some(*compact_revision.peek()));
                                                    compact_result.set(None);
                                                }
                                            }
                                        } else {
                                            node_compacting.set(false);
                                            compact_revision += 1;
                                        }
                                    }
                                    if matches!(
                                        event_type,
                                        "auto_compaction_end" | "compaction_end"
                                    ) {
                                        node_compacting.set(false);
                                        compact_revision += 1;
                                    }
                                    if event_type == "agent_settled" {
                                        let usage = event
                                            .wire
                                            .data
                                            .get("usage")
                                            .filter(|v| v.is_object())
                                            .cloned();
                                        last_turn_usage.set(
                                            usage.clone().map(|usage| (trace_scope.clone(), usage)),
                                        );
                                        let outcome = event
                                            .wire
                                            .data
                                            .get("outcome")
                                            .and_then(Value::as_str)
                                            .unwrap_or("unknown")
                                            .to_owned();
                                        // A compacted settlement is authoritative even
                                        // without a local request/accepted event. It is
                                        // also its own epoch boundary, so late branch
                                        // reads from before settlement are discarded.
                                        if outcome == "compacted" {
                                            let summary = event
                                                .wire
                                                .data
                                                .get("summary")
                                                .and_then(Value::as_str)
                                                .filter(|text| !text.trim().is_empty())
                                                .map(str::to_owned);
                                            let error = event
                                                .wire
                                                .data
                                                .get("error")
                                                .and_then(Value::as_str)
                                                .or_else(|| {
                                                    event
                                                        .wire
                                                        .data
                                                        .get("message")
                                                        .and_then(Value::as_str)
                                                })
                                                .filter(|text| !text.trim().is_empty())
                                                .map(str::to_owned);
                                            compact_result.set(Some(CompactResult {
                                                outcome: outcome.clone(),
                                                summary,
                                                error,
                                            }));
                                            compact_revision += 1;
                                            compact_pending.set(None);
                                            compact_operation_accepted.set(false);
                                            node_compacting.set(false);
                                            status.set("上下文压缩已完成，摘要已更新。".into());
                                        } else if compact_pending.peek().is_some()
                                            && *compact_operation_accepted.peek()
                                        {
                                            let summary = event
                                                .wire
                                                .data
                                                .get("summary")
                                                .and_then(Value::as_str)
                                                .filter(|text| !text.trim().is_empty())
                                                .map(str::to_owned);
                                            let error = event
                                                .wire
                                                .data
                                                .get("error")
                                                .and_then(Value::as_str)
                                                .or_else(|| {
                                                    event
                                                        .wire
                                                        .data
                                                        .get("message")
                                                        .and_then(Value::as_str)
                                                })
                                                .filter(|text| !text.trim().is_empty())
                                                .map(str::to_owned);
                                            compact_result.set(Some(CompactResult {
                                                outcome: outcome.clone(),
                                                summary,
                                                error,
                                            }));
                                            compact_pending.set(None);
                                            compact_operation_accepted.set(false);
                                            node_compacting.set(false);
                                            status.set(
                                                match outcome.as_str() {
                                                    "declined" => "节点拒绝了压缩请求。",
                                                    "aborted" => "压缩请求已中止。",
                                                    "failed" => "上下文压缩失败。",
                                                    _ => "压缩请求已结算，但结果未知。",
                                                }
                                                .into(),
                                            );
                                        }
                                    }
                                    if matches!(
                                        event.wire.event_type.as_str(),
                                        "agent_settled"
                                            | "connection_online"
                                            | "auto_compaction_end"
                                            | "compaction_end"
                                            | "compaction_progress"
                                    ) {
                                        let refresh_client = client.clone();
                                        let refresh_session = session_id.clone();
                                        state::scoped_spawn(requests, async move {
                                            if let Ok(snapshot) =
                                                refresh_client.queue(&refresh_session, false).await
                                            {
                                                if *session_generation.read() == generation {
                                                    queue.set(Some(snapshot));
                                                }
                                            }
                                            if let Ok(state) =
                                                refresh_client.runtime_state(&refresh_session).await
                                            {
                                                if *session_generation.read() == generation {
                                                    runtime_state.set(Some(state));
                                                }
                                            }
                                            if let Ok(items) = refresh_client.sessions().await {
                                                if *session_generation.read() == generation {
                                                    sessions.set(items);
                                                }
                                            }
                                        });
                                    }
                                }
                                Ok(None) => {
                                    status.set("SSE 已关闭 · 会话状态需重新同步".into());
                                    break;
                                }
                                Err(error) => {
                                    status.set(format!("会话流错误：{error}"));
                                    break;
                                }
                            }
                        }
                    }
                    Err(error) => status.set(format!("打开会话失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let mut open_project = move |project_id: String| {
        let recent = sessions
            .read()
            .iter()
            .filter(|s| s.project_id.as_deref() == Some(project_id.as_str()))
            .max_by_key(|s| s.updated_at)
            .map(|s| s.session_id.clone());
        if let Some(id) = recent {
            open_session(id);
            return;
        }
        state::cancel_requests(requests);
        session_generation += 1;
        pending_dialogs.set(Vec::new());
        sidebar_open.set(false);
        selected_session.set(None);
        selected_session_title.set(None);
        transcript.set(Vec::new());
        projection.set(None);
        queue.set(None);
        runtime_state.set(None);
        pending_dialogs.set(Vec::new());
        active_page.set("workspace".to_owned());
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let generation = *file_generation.read() + 1;
        file_generation.set(generation);
        if let Some(project) = projects
            .read()
            .iter()
            .find(|project| project.id == project_id)
        {
            let scope = project_scope_key(&node_origin(node_url.read().as_str()), &project.id);
            let name = project_aliases
                .read()
                .get(&scope)
                .cloned()
                .unwrap_or_else(|| project.name.clone());
            selected_project_name.set(Some(name));
            selected_project_path.set(Some(project.path.clone()));
        } else {
            selected_project_name.set(None);
            selected_project_path.set(None);
        }
        selected_project.set(Some(project_id.clone()));
        files.set(None);
        file_content.set(None);
        git_status.set(None);
        selected_project_file.set(None);
        selected_change_file.set(None);
        change_diff.set(None);
        change_error.set(None);
        status.set("正在加载项目文件…".into());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => {
                    // Directory browsing is deliberately on-demand. Do not recurse through
                    // the project just to populate a client-side catalogue.
                    let files_result = client.files(&project_id, "").await;
                    let git_result = client.git_status(&project_id).await;
                    if *file_generation.read() != generation {
                        return;
                    }
                    match files_result {
                        Ok(listing) => files.set(Some(listing)),
                        Err(error) => status.set(format!("读取项目文件失败：{error}")),
                    }
                    if let Ok(repository) = git_result {
                        git_status.set(Some(repository));
                    }
                    status.set("项目文件已加载".into());
                }
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    use_effect(move || {
        let pending = pending_open.read().clone();
        if let Some((project, id)) = pending {
            pending_open.set(None);
            if project {
                open_project(id);
            } else {
                open_session(id);
            }
        }
    });

    let mut open_file = move |path: String| {
        let Some(project_id) = selected_project.read().clone() else {
            return;
        };
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        project_file_preview.set(true);
        project_file_mode.set("preview".into());
        file_loading.set(true);
        file_error.set(None);
        let generation = *file_generation.read() + 1;
        file_generation.set(generation);
        selected_project_file.set(Some(path.clone()));
        file_content.set(None);
        status.set("正在加载文件内容…".into());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.file(&project_id, &path).await {
                    Ok(content) if *file_generation.read() == generation => {
                        file_loading.set(false);
                        file_content.set(Some(content));
                        status.set("文件内容已加载".into());
                    }
                    Err(error) if *file_generation.read() == generation => {
                        file_loading.set(false);
                        file_error.set(Some(error.to_string()));
                        status.set(format!("读取文件失败：{error}"));
                    }
                    _ => {}
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let mut quote_project_file = move |path: String| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话，再引用项目文件".into());
            return;
        };
        let draft_scope = session_scope_key(&node_url.read(), &session_id);
        let existing = drafts
            .read()
            .get(&draft_scope)
            .map(|draft| draft.text.clone())
            .unwrap_or_default();
        let reference = format!("@{path}");
        let text = if existing.is_empty() {
            reference
        } else {
            format!("{existing}\n{reference}")
        };
        update_draft(drafts, &draft_scope, text);
        status.set("文件路径已加入当前草稿".into());
    };

    let mut open_change = move |path: String| {
        let Some(project_id) = selected_project.read().clone() else {
            return;
        };
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        change_mobile_preview.set(true);
        let generation = *change_generation.read() + 1;
        change_generation.set(generation);
        selected_change_file.set(Some(path.clone()));
        change_diff.set(None);
        change_error.set(None);
        status.set("正在加载 Git patch…".into());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.diff(&project_id, &path, false).await {
                    Ok(patch) if *change_generation.read() == generation => {
                        change_diff.set(Some(patch));
                        status.set("Git patch 已加载".into());
                    }
                    Err(error) if *change_generation.read() == generation => {
                        change_error.set(Some(error.to_string()));
                        status.set(format!("读取 Git patch 失败：{error}"));
                    }
                    _ => {}
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let mut send_prompt = move || {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        let draft_scope = session_scope_key(&node_url.read(), &session_id);
        if sending.read().contains(&draft_scope) {
            return;
        }
        let draft = state::draft_for(&drafts.read(), &draft_scope);
        if draft.text.trim().is_empty() {
            return;
        }
        let Some(client) = shared_client.read().clone() else {
            return;
        };
        // The draft is the complete prompt. Desired resources are immutable client
        // trace metadata only; node tools are never guessed or injected here.
        let scope = session_scope_key(&node_url.read(), &session_id);
        let binding = resource_library
            .read()
            .sessions
            .iter()
            .find(|item| item.session_id == scope)
            .cloned()
            .unwrap_or_else(|| resources::SessionBinding {
                session_id: scope.clone(),
                revision: 1,
                ..Default::default()
            });
        let resource_snapshot = resource_snapshot(&binding, &resource_library.read());
        let trace_watermark = traces::live_run_watermark(traces, &scope);
        let prompt_text = draft.text.clone();
        sending.write().insert(draft_scope.clone());
        let generation = *session_generation.read();
        state::scoped_spawn(submissions, async move {
            let result = client.prompt(&session_id, &prompt_text).await;
            sending.write().remove(&draft_scope);
            match result {
                Ok(response) if response.success => {
                    traces::confirm_resource_snapshot(
                        traces,
                        &scope,
                        trace_watermark,
                        resource_snapshot,
                    );
                    let unchanged = drafts
                        .read()
                        .get(&draft_scope)
                        .is_some_and(|d| d.revision == draft.revision);
                    if unchanged {
                        update_draft(drafts, &draft_scope, String::new());
                    }
                    if *session_generation.read() == generation {
                        status.set("指令已接收，等待事件流…".into());
                    }
                }
                _ if *session_generation.read() != generation => {}
                Ok(response) => status.set(format!(
                    "指令未接受：{}",
                    response.error.unwrap_or_else(|| "未知错误".into())
                )),
                Err(error) => status.set(format!("发送失败：{error}")),
            }
        });
    };

    let mut deliver_composer = move || {
        let Some(session_id) = selected_session.read().clone() else {
            return;
        };
        let scope = session_scope_key(&node_url.read(), &session_id);
        if sending.read().contains(&scope) {
            return;
        }
        let draft = state::draft_for(&drafts.read(), &scope);
        if draft.text.trim().is_empty() {
            return;
        }
        let Some(client) = shared_client.read().clone() else {
            status.set("请先连接执行节点；草稿已保留。".into());
            return;
        };
        let message = draft.text.clone();
        let delivery = queue_delivery.read().clone();
        let generation = *session_generation.read();
        sending.write().insert(scope.clone());
        status.set("正在投递消息，等待节点确认…".into());
        state::scoped_spawn(submissions, async move {
            let result = client.queue_message(&session_id, &message, &delivery).await;
            sending.write().remove(&scope);
            match result {
                Ok(response) if response.accepted == Some(true) => {
                    if drafts
                        .read()
                        .get(&scope)
                        .is_some_and(|current| current.revision == draft.revision)
                    {
                        update_draft(drafts, &scope, String::new());
                    }
                    if let Ok(snapshot) = client.queue(&session_id, false).await {
                        if *session_generation.read() == generation {
                            queue.set(Some(snapshot));
                        }
                    }
                    if *session_generation.read() == generation {
                        status.set(format!(
                            "消息已被节点接受并加入{}队列。",
                            if delivery == "steer" {
                                "引导"
                            } else {
                                "后续"
                            }
                        ));
                    }
                }
                Ok(_) => {
                    if *session_generation.read() == generation {
                        status.set("节点未确认消息已入队；草稿已保留，请检查队列。".into());
                    }
                }
                Err(error) => {
                    if *session_generation.read() == generation {
                        status.set(format!("投递失败：{error}；草稿已保留。"));
                    }
                }
            }
        });
    };

    let mut send_or_command = move || {
        let text = selected_session
            .read()
            .as_ref()
            .map(|id| session_scope_key(&node_url.read(), id))
            .and_then(|scope| drafts.read().get(&scope).map(|draft| draft.text.clone()))
            .unwrap_or_default();
        let command = text.trim();
        if let Some(question) = command
            .strip_prefix("/btw")
            .filter(|tail| tail.chars().next().is_none_or(char::is_whitespace))
        {
            if let Some(id) = selected_session.read().clone() {
                let scope = session_scope_key(&node_url.read(), &id);
                composer_extras::set_btw_draft(btw_store, &scope, question.trim().to_owned());
                btw_open.set(true);
            }
            return;
        }
        if command == "/compact" {
            context_open.set(true);
            return;
        }
        if command == "/new" {
            created_session_id.set(None);
            new_session_error.set(None);
            new_session_open.set(true);
            return;
        }
        if command == "/help" {
            status.set("可用命令：/btw [问题]、/compact、/new、/help。".into());
            return;
        }
        if command.starts_with('/') {
            status.set("未知命令。可用：/btw [问题]、/compact、/new、/help。草稿已保留。".into());
            return;
        }
        let is_running = projection
            .read()
            .as_ref()
            .is_some_and(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_)));
        if is_running {
            deliver_composer();
        } else {
            send_prompt();
        }
    };

    use_effect(move || {
        let intent = pending_enter.read().clone();
        let busy = sending.read();
        let generation = *session_generation.read();
        let current_scope = selected_session
            .read()
            .as_ref()
            .map(|id| session_scope_key(&node_url.read(), id));
        let Some((scope, revision, expected_generation)) = intent else {
            return;
        };
        if generation != expected_generation
            || current_scope.as_ref() != Some(&scope)
            || drafts
                .read()
                .get(&scope)
                .is_none_or(|draft| draft.revision != revision)
        {
            pending_enter.set(None);
            return;
        }
        if busy.contains(&scope) {
            return;
        }
        drop(busy);
        pending_enter.set(None);
        send_or_command();
    });

    let abort = move |_| {
        let Some(session_id) = selected_session.read().clone() else {
            return;
        };
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.abort(&session_id).await {
                    Ok(_) => status.set("停止请求已发送，等待 agent_settled…".into()),
                    Err(error) => status.set(format!("停止失败：{error}")),
                },
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };
    let answer_dialog = move |answer: conversation::DialogAnswer| {
        let Some(session_id) = selected_session.read().clone() else {
            return;
        };
        let request_id = answer.request_id.clone();
        let scope = session_scope_key(&node_url.read(), &session_id);
        let key = format!("{scope}::{request_id}");
        if dialog_busy.read().contains(&key) {
            return;
        }
        dialog_busy.write().insert(key.clone());
        dialog_errors.write().remove(&key);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            let result = match client_result {
                Ok(client) => match answer.answer {
                    Some(value) => client
                        .dialog_response(&session_id, &request_id, value)
                        .await
                        .map(|_| ()),
                    None => client
                        .cancel_dialog(&session_id, &request_id)
                        .await
                        .map(|_| ()),
                },
                Err(error) => Err(error),
            };
            dialog_busy.write().remove(&key);
            match result {
                Ok(()) => {
                    answered_dialogs.write().insert(key.clone());
                    pending_dialogs
                        .write()
                        .retain(|item| item.request_id != request_id);
                    status.set("确认请求已提交，等待节点结算…".into());
                }
                Err(error) => {
                    dialog_errors
                        .write()
                        .insert(key, format!("提交失败：{error}；已保留你的选择，可重试。"));
                    status.set(format!("确认请求失败：{error}"));
                }
            }
        });
    };

    let composer = selected_session
        .read()
        .as_ref()
        .map(|id| session_scope_key(&node_url.read(), id))
        .and_then(|scope| drafts.read().get(&scope).map(|d| d.text.clone()))
        .unwrap_or_default();
    let submitting = selected_session.read().as_ref().is_some_and(|id| {
        sending
            .read()
            .contains(&session_scope_key(&node_url.read(), id))
    });
    let running = submitting
        || projection.read().as_ref().is_some_and(|run| {
            matches!(
                &run.phase,
                RunPhase::Accepted
                    | RunPhase::Preparing
                    | RunPhase::Text
                    | RunPhase::Tool
                    | RunPhase::AwaitingInput
                    | RunPhase::AwaitingSettlement
                    | RunPhase::Cancelling
            )
        });
    let query = search.read().trim().to_lowercase();
    let visible_sessions = sessions
        .read()
        .iter()
        .filter(|session| {
            query.is_empty()
                || session
                    .title
                    .as_deref()
                    .unwrap_or("未命名会话")
                    .to_lowercase()
                    .contains(&query)
                || session.status.to_lowercase().contains(&query)
        })
        .cloned()
        .collect::<Vec<_>>();
    let visible_projects = projects
        .read()
        .iter()
        .filter(|project| {
            query.is_empty()
                || project.name.to_lowercase().contains(&query)
                || project.path.to_lowercase().contains(&query)
        })
        .cloned()
        .collect::<Vec<_>>();
    let repository_snapshot = git_status.read().clone();
    let reviewed_snapshot = reviewed_changes.read().clone();
    let visible_changes = repository_snapshot
        .as_ref()
        .map(|repository| {
            filter_changed_files(
                &repository.files,
                change_query.read().as_str(),
                change_status_filter.read().as_str(),
                change_directory_filter.read().as_str(),
                *changes_unreviewed.read(),
                &reviewed_snapshot,
            )
        })
        .unwrap_or_default();
    let reviewed_count = repository_snapshot
        .as_ref()
        .map(|repository| {
            repository
                .files
                .iter()
                .filter(|file| reviewed_snapshot.contains(&file.path))
                .count()
        })
        .unwrap_or_default();
    let project_for_name = editing_project_id
        .read()
        .as_ref()
        .and_then(|id| projects.read().iter().find(|item| &item.id == id).cloned());

    let meta_inspector = {
        let store = meta_store.read();
        let session = selected_session.read().clone();
        let association = session.as_ref().and_then(|id| {
            development_associations
                .read()
                .get(&session_scope_key(
                    &node_origin(node_url.read().as_str()),
                    id,
                ))
                .cloned()
        });
        if active_page.read().as_str() == "meta-agent" {
            selected_project.read().as_ref().and_then(|project| {
                let resource = store
                    .selected_by_project
                    .get(project)
                    .and_then(|id| {
                        store
                            .resources
                            .iter()
                            .find(|resource| resource.project_id == *project && resource.id == *id)
                    })
                    .or_else(|| {
                        store
                            .resources
                            .iter()
                            .find(|resource| resource.project_id == *project)
                    })?;
                Some((project.clone(), Some(resource.clone()), None))
            })
        } else if let Some(association) = association {
            session
                .as_ref()
                .and_then(|id| {
                    sessions
                        .read()
                        .iter()
                        .find(|session| session.session_id == *id)
                        .and_then(|session| session.project_id.clone())
                })
                .map(|project| {
                    let resource_id = association
                        .rsplit_once("#r")
                        .and_then(|(identity, _)| identity.rsplit_once('@'))
                        .map(|(id, _)| id);
                    let resource = store
                        .resources
                        .iter()
                        .find(|resource| {
                            resource.project_id == project
                                && Some(resource.id.as_str()) == resource_id
                        })
                        .cloned();
                    (project, resource, Some(association))
                })
        } else {
            None
        }
    };
    let workspace_page = active_page.read().as_str() == "workspace";
    let navigation_title = match active_page.read().as_str() {
        "packages" => "Packages",
        "skills" => "技能库",
        "extensions" => "扩展",
        "meta-agent" => "元Agent",
        "settings" => match settings_section.read().as_str() {
            "devices" => {
                if device_mode.read().is_mobile() {
                    "连接我的电脑"
                } else {
                    "执行节点"
                }
            }
            "providers" => "Providers",
            "updates" => "应用更新",
            _ => "通用",
        },
        _ => "工作空间",
    };
    let inspector_context = runtime_state.read().as_ref().map_or_else(
        || "— / —".into(),
        |runtime| {
            let context = runtime_stats::Context::reported(runtime.context.as_ref());
            format!(
                "{} / {}",
                runtime_stats::tokens(context.tokens),
                runtime_stats::tokens(context.window)
            )
        },
    );
    let inspector_provider = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.provider.clone())
        .unwrap_or_else(|| "节点未返回".into());
    let inspector_model = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.model_id.clone())
        .unwrap_or_else(|| "节点未返回".into());
    let inspector_thinking = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.thinking_level.clone())
        .unwrap_or_else(|| "节点未返回".into());
    let inspector_plan = conversation::plan_for_session(
        &transcript.read(),
        projection.read().as_ref().map(|run| run.blocks.as_slice()),
    );
    let inspector_context_percent = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime_stats::Context::reported(runtime.context.as_ref()).percent);
    let inspector_threshold = runtime_state.read().as_ref().and_then(|runtime| {
        let context = runtime_stats::Context::reported(runtime.context.as_ref());
        context
            .threshold
            .zip(context.window)
            .filter(|(_, window)| *window > 0.0)
            .map(|(threshold, window)| threshold / window * 100.0)
    });

    rsx! {
        updates::UpdateNotice { onopen: move |()| { active_page.set("settings".to_owned()); settings_section.set("updates".to_owned()); } }
            style { "{THEMES}" }
            style { "{CSS}" }
            style { "{RESOURCES_CSS}" }
            style { "{META_AGENT_CSS}" }
            style { "{PROJECT_PICKER_CSS}" }
            style { "{TRACES_CSS}" }
            style { "{CONVERSATION_CSS}" }
            style { "{EXTRAS_CSS}" }
            style { "{QUEUE_CSS}" }
            style { "{runtime_stats::CSS}" }
            div { class: "prototype-bar",
                span { "●  RPI WORK / 真实客户端" }
                span { "真实节点 · SSE 会话" }
                span { "Dioxus" }
            }
            runtime_stats::RuntimeDialogs {
                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                runtime_state, projection, transcript, last_turn: last_turn_usage,
                shared_client, selected_session, session_generation, compact_revision, compact_pending, node_compacting,
                compact_result, pending_ask: !pending_dialogs.read().is_empty(), stats_open, context_open,
                ontools: move |()| session_details_open.set(true),
            }
            if *queue_open.read() {
                if let Some(session_id) = selected_session.read().clone() {
                    { let scope = session_scope_key(&node_url.read(), &session_id); let expected_scope = scope.clone(); rsx! {
                        message_queue::QueueDialog {
                            scope, session_id: session_id.clone(), client: shared_client, drafts: queue_drafts,
                            snapshot: queue.read().clone(),
                            onchange: move |snapshot| {
                                if selected_session.read().as_ref().is_some_and(|id| session_scope_key(&node_url.read(), id) == expected_scope) { queue.set(Some(snapshot)); }
                            },
                            onclose: move |()| queue_open.set(false),
                        }
                    } }
                }
            }
            main { class: format!("{}{}", match (active_page.read().as_str(), active_view.read().as_str(), *inspector_open.read()) {
                ("workspace", "changes", _) => "app-shell review-mode",
                ("workspace", _, false) => "app-shell context-hidden",
                _ => "app-shell",
            }, if *btw_open.read() && active_page.read().as_str() == "workspace" { " btw-open" } else { "" }), onkeydown: move |event| {
                if event.key() == Key::Escape && *sidebar_open.read() { sidebar_open.set(false); }
            },
                if *sidebar_open.read() {
                    button { class: "scrim", aria_label: "关闭项目导航", onclick: move |_| sidebar_open.set(false) }
                }
                aside { class: if *sidebar_open.read() { "sidebar open" } else { "sidebar" },
                    a { class: "brand", href: "#",
                        span { class: "brand-mark", "r" span { "↗" } }
                        strong { "rpi" span { "work" } }
                        span { class: "version", "MVP" }
                    }
                    button { class: "search-trigger", onclick: move |_| { search.set(String::new()); global_search_open.set(true); },
                        Icon { name: "search" } span { "搜索项目与会话" } kbd { "⌘ K" }
                    }
                    nav { class: "primary-nav",
                        for (page, icon, title) in [("workspace", "layout", "工作空间"), ("extensions", "blocks", "扩展"), ("packages", "blocks", "Packages"), ("skills", "skills", "技能库")] {
                            button { class: if active_page.read().as_str() == page { "nav-item active" } else { "nav-item" }, onclick: move |_| { active_page.set(page.to_owned()); sidebar_open.set(false); },
                                Icon { name: icon.to_owned() } "{title}"
                            }
                        }
                    }
                    div { class: "section-label", "项目" button { aria_label: "添加项目", onclick: move |_| { project_error.set(None); show_project_form.set(true); }, Icon { name: "plus".to_owned() } } }
                    if let Some(project_id) = selected_project.read().as_ref() {
                        if project_purposes
                            .read()
                            .get(&project_scope_key(&node_origin(node_url.read().as_str()), project_id))
                            .is_some_and(|purpose| purpose == "元Agent") {
                            button { class: if active_page.read().as_str() == "meta-agent" { "nav-item active" } else { "nav-item" }, onclick: move |_| { meta_return_page.set("workspace".into()); active_page.set("meta-agent".into()); sidebar_open.set(false); }, Icon { name: "blocks" } "元Agent" }
                        }
                    }
                    if visible_projects.is_empty() {
                        div { class: "project-row selected", span { class: "project-initial", "W" } span { if query.is_empty() { "尚未添加项目" } else { "没有匹配项目" } } }
                    } else {
                        for (index, project) in visible_projects.iter().enumerate() {
                            {
                                let project_id = project.id.clone();
                                let open_project_id = project_id.clone();
                                let active = selected_project.read().as_ref() == Some(&project_id);
                                let project_sessions = visible_sessions
                                    .iter()
                                    .filter(|session| session.project_id.as_deref() == Some(project_id.as_str()))
                                    .cloned()
                                    .collect::<Vec<_>>();
                                rsx! {
                                    div { class: "project-entry",
                                    button {
                                        key: "sidebar-project-{project_id}",
                                        class: if active { "project-row selected" } else { "project-row" },
                                        onclick: move |_| open_project(open_project_id.clone()),
                                        oncontextmenu: { let project_id = project_id.clone(); move |event| { event.prevent_default(); editing_project_id.set(Some(project_id.clone())); project_name_error.set(None); project_name_open.set(true); } },
                                        span { class: if index == 0 { "project-initial" } else { "project-initial mint" }, "{project.name.chars().next().unwrap_or('P')}" }
                                        span { "{project_aliases.read().get(&project_scope_key(&node_origin(node_url.read().as_str()), &project.id)).cloned().unwrap_or_else(|| project.name.clone())}" }
                                        if active { Icon { name: "chevron".to_owned() } }
                                    }
                                    button {
                                        class: "project-row-menu",
                                        r#type: "button",
                                        aria_label: "编辑项目显示名称",
                                        title: "编辑显示名称",
                                        onclick: { let project_id = project.id.clone(); move |_| { editing_project_id.set(Some(project_id.clone())); project_name_error.set(None); project_name_open.set(true); } },
                                        Icon { name: "more".to_owned() }
                                    }
                                    if active {
                                        div { class: "sidebar-sessions", aria_label: "会话列表",
                                            for session in project_sessions.iter() {
                                                {
                                                    let id = session.session_id.clone();
                                                    let selected = selected_session.read().as_ref() == Some(&id);
                                                    rsx! {
                                                        button {
                                                            key: "sidebar-session-{id}",
                                                            class: if selected { "session-item current" } else { "session-item" },
                                                            onclick: move |_| open_session(id.clone()),
                                                            span { class: "session-dot" }
                                                            span { "{session.title.as_deref().unwrap_or(\"未命名会话\")}" }
                                                        }
                                                    }
                                                }
                                            }
                                            if project_sessions.is_empty() {
                                                span { class: "sidebar-empty", if query.is_empty() { "连接后显示会话" } else { "没有匹配会话" } }
                                            }
                                        }
                                    }
                                    }
                                }
                            }
                        }
                    }
                    if visible_sessions.iter().any(|session| session.project_id.is_none()) {
                        div { class: "section-label", "独立会话" }
                        div { class: "sidebar-sessions",
                            for session in visible_sessions.iter().filter(|session| session.project_id.is_none()) {
                                {
                                    let id = session.session_id.clone();
                                    let selected = selected_session.read().as_ref() == Some(&id);
                                    rsx! {
                                        button { class: if selected { "session-item current" } else { "session-item" }, onclick: move |_| open_session(id.clone()),
                                            span { class: "session-dot" } span { "{session.title.as_deref().unwrap_or(\"未命名会话\")}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    button { class: "new-task", onclick: move |_| { created_session_id.set(None); new_session_error.set(None); new_session_open.set(true); }, Icon { name: "plus".to_owned() } "新建任务" }
                    div { class: "sidebar-spacer" }
                    button { class: "profile", aria_label: "打开设置", onclick: move |_| { active_page.set("settings".to_owned()); settings_section.set("settings".to_owned()); sidebar_open.set(false); },
                        span { class: "avatar", "R" }
                        span { strong { "设置" } small { "连接、Provider 与外观" } }
                        Icon { name: "settings".to_owned() }
                    }
                }
                div { class: "main-column",
                    header { class: "app-header",
                        button { class: "mobile-menu", aria_label: "打开项目导航", onclick: move |_| sidebar_open.set(true), Icon { name: "menu".to_owned() } }
                        div { class: if workspace_page { "breadcrumb" } else { "breadcrumb page-breadcrumb" },
                            Icon { name: if active_page.read().as_str() == "settings" { "settings" } else { "folder" } }
                            span { class: "breadcrumb-project",
                                if active_page.read().as_str() == "settings" { "设置" }
                                else { "{selected_project_name.read().as_deref().unwrap_or(\"rpi-work\")}" }
                            }
                            span { class: "slash", "/" }
                            h1 {
                                if workspace_page {
                                    if let Some(title) = selected_session_title.read().as_deref() { "{title}" } else { "未选择会话" }
                                } else { "{navigation_title}" }
                            }
                        }
                        if workspace_page {
                            button { class: "quick-session", aria_label: "切换会话", onclick: move |_| { picker_search.set(String::new()); session_switcher_open.set(true); },
                                span {
                                    small { "{selected_project_name.read().as_deref().unwrap_or(\"rpi-work\")}" }
                                    strong { "{selected_session_title.read().as_deref().unwrap_or(\"选择一个会话\")}" }
                                }
                                Icon { name: "chevron".to_owned() }
                            }
                        }
                        div { class: "top-actions",
                            button { class: "theme-trigger", aria_label: "外观与主题", onclick: move |_| appearance_open.set(true), Icon { name: "sparkles".to_owned() } span { "{appearance.read().theme.name()}" } }
                            span { class: if projection.read().is_some() { "status-badge running" } else { "status-badge waiting" },
                                i {}
                                "{projection_status(projection.read().as_ref(), status.read().as_str())}"
                            }
                            if let Some(session_id) = selected_session.read().as_ref() {
                                if development_associations
                                    .read()
                                    .contains_key(&session_scope_key(&node_origin(node_url.read().as_str()), session_id)) {
                                    button { class: "tab-resource", onclick: move |_| { active_page.set("meta-agent".into()); }, "回到元Agent源稿" }
                                }
                            }
                            button { class: "context-toggle", aria_label: "查看任务上下文", onclick: move |_| {
                                let current = *inspector_open.read();
                                inspector_open.set(!current);
                            }, Icon { name: "panel".to_owned() } }
                        }
                    }
                if *show_project_form.read() {
                    project_picker::ProjectPicker {
                        shared_client, projects: projects.read().to_vec(),
                        busy: *project_busy.read(), error: project_error.read().clone(),
                        onsubmit: submit_project,
                        onclose: move |()| { project_generation += 1; state::cancel_requests(project_requests); project_busy.set(false); project_error.set(None); show_project_form.set(false); },
                    }
                }
                if *project_name_open.read() {
                    if let Some(project) = project_for_name.clone() {
                        project_picker::ProjectNameDialog {
                            key: "project-name-{project.id}",
                            project: work_client::Project { name: project_aliases.read().get(&project_scope_key(&node_origin(node_url.read().as_str()), &project.id)).cloned().unwrap_or(project.name), ..project },
                            busy: false, error: project_name_error.read().clone(),
                            onsave: save_project_alias,
                            onclose: move |()| { project_name_open.set(false); editing_project_id.set(None); project_name_error.set(None); },
                        }
                    }
                }
                if *new_session_open.read() {
                    project_picker::NewSessionDialog {
                        projects: projects.read().to_vec(), current_project: selected_project.read().clone(),
                        busy: *new_session_busy.read(), error: new_session_error.read().clone(),
                        onsubmit: submit_new_session,
                        onclose: move |()| new_session_open.set(false),
                    }
                }
                if ["packages", "skills", "extensions"].contains(&active_page.read().as_str()) {
                    div { class: "page-view", resources::ResourcePage {
                        kind: active_page.read().clone(), library: resource_library,
                        session_name: selected_session_title.read().clone(),
                        session_scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)),
                        on_back: Some(EventHandler::new(move |()| active_page.set("workspace".to_owned()))),
                    } }
                }
                if active_page.read().as_str() == "meta-agent" {
                    if let (Some(project_id), Some(project_name)) = (selected_project.read().clone(), selected_project_name.read().clone()) {
                        if project_purposes
                            .read()
                            .get(&project_scope_key(&node_origin(node_url.read().as_str()), &project_id))
                            .is_some_and(|purpose| purpose == "元Agent") {
                            meta_agent::MetaAgentPage {
                                project_id: project_id.clone(), project_name,
                                store: meta_store,
                                ondevelop: move |resource: meta_agent::MetaResource| {
                                    let Some(client) = shared_client.read().clone() else { status.set("请先连接节点再创建开发会话".into()); return; };
                                    let project_id = Some(resource.project_id.clone());
                                    let title = format!("开发：{} · {}", resource.name, resource.version);
                                    let drafts = drafts;
                                    let mut pending_open = pending_open;
                                    let mut sessions = sessions;
                                    let mut status = status;
                                    let mut active_page = active_page;
                                    let mut session_title_draft = session_title_draft;
                                    let mut association = development_associations;
                                    let origin = node_origin(node_url.read().as_str());
                                    state::scoped_spawn(requests, async move {
                                        match client.create_session(project_id.as_deref()).await {
                                            Ok(session_id) => match client.rename_session(&session_id, &title).await {
                                                Ok(_) => {
                                                    let source = resource.sources.iter().map(|(path, text)| format!("--- {path} ---\n{text}")).collect::<Vec<_>>().join("\n\n");
                                                    let text = format!("继续开发元Agent资源 {}（ID: {}，revision: {}，版本: {}）。\n\n源码是客户端设计草稿；节点修改项目文件不会自动写回此草稿。完成修改后请显式回到元Agent资源编辑器保存。当前测试/发布快照保持不变。\n\n完整所选源码快照：\n{}", resource.name, resource.id, resource.revision, resource.version, source);
                                                    let scope = session_scope_key(&origin, &session_id);
                                                    update_draft(drafts, &scope, text);
                                                    association.write().insert(session_scope_key(&origin, &session_id), format!("{}@{}#r{}", resource.id, resource.version, resource.revision));
                                                    if let Ok(items) = client.sessions().await { sessions.set(items); }
                                                    session_title_draft.set(String::new());
                                                    active_page.set("workspace".into());
                                                    pending_open.set(Some((false, session_id)));
                                                    status.set("已创建元Agent开发会话；源码草稿未自动写回资源库。".into());
                                                }
                                                Err(error) => status.set(format!("会话已创建但命名失败：{error}")),
                                            },
                                            Err(error) => status.set(format!("创建元Agent开发会话失败：{error}")),
                                        }
                                    });
                                },
                                onpublish: Callback::new(move |value: Value| {
                                    resources::import_design_package(&mut resource_library.write(), &value)
                                }),
                            }
                        } else { div { class: "page-view", p { "请选择元Agent项目。" } } }
                    } else { div { class: "page-view", p { "请先选择元Agent项目。" } } }
                }
                if active_page.read().as_str() == "settings" {
                    SettingsPage { shared_client, requests, active_page, settings_section, base_url, status, auth_state, password, runtime_state, capabilities, selected_session, session_generation,
                        mobile_client: device_mode.read().is_mobile(),
                        update_busy: !sending.read().is_empty() || projection.read().as_ref().is_some_and(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_))) || drafts.read().values().any(|draft| !draft.text.trim().is_empty()) || attachment_store.read().has_pending_content() || btw_store.read().has_pending_content() || queue_drafts.read().has_pending_content(),
                        onconnect: connect, onlogin: login, onappearance: move |()| appearance_open.set(true)
                    }
                }
                div { class: match (active_page.read().as_str(), *inspector_open.read(), active_view.read().as_str()) {
                    ("workspace", true, "changes") => "workspace review-mode",
                    ("workspace", true, _) => "workspace",
                    ("workspace", false, "changes") => "workspace context-hidden review-mode",
                    ("workspace", false, _) => "workspace context-hidden",
                    _ => "workspace page-hidden",
                },
                    section { class: "conversation-panel",
                        div { class: "section-tabs",
                            button {
                                class: if active_view.read().as_str() == "conversation" { "active" } else { "" },
                                onclick: move |_| active_view.set("conversation".to_owned()),
                                Icon { name: "message".to_owned() } "对话"
                            }
                            button {
                                class: if active_view.read().as_str() == "changes" { "active" } else { "" },
                                onclick: move |_| active_view.set("changes".to_owned()),
                                Icon { name: "diff".to_owned() } "变更"
                                if let Some(repository) = git_status.read().as_ref() {
                                    span { class: "tab-count", "{repository.files.len()}" }
                                }
                            }
                            button { class: "tab-resource", aria_label: "打开项目文件", onclick: move |_| project_files_open.set(true), Icon { name: "file".to_owned() } span { class: "resource-label", "项目文件" } }
                            if active_view.read().as_str() == "conversation" {
                                if let Some(session_id) = selected_session.read().clone() {
                                    resources::SessionResources {
                                        session_id: session_scope_key(&node_url.read(), &session_id), compact: true, open_requested: open_session_config,
                                        session_title: selected_session_title.read().clone(),
                                        on_browse: Some(EventHandler::new(move |()| active_page.set("packages".to_owned()))),
                                        selected_session,
                                        library: resource_library, onquote: move |text: String| {
                                            if let Some(id) = selected_session.read().as_deref() {
                                                let scope = session_scope_key(&node_url.read(), id);
                                                let existing = drafts.read().get(&scope).map(|draft| draft.text.clone()).unwrap_or_default();
                                                let text = if existing.is_empty() { text } else { format!("{existing}\n{text}") };
                                                update_draft(drafts, &scope, text);
                                            }
                                        }, shared_client, session_generation, requests, runtime_state, status,
                                    }
                                }
                            }
                        }
                        if active_view.read().as_str() == "changes" {
                            div { class: if *change_mobile_preview.read() { "change-workspace show-diff" } else { "change-workspace" },
                                div { class: "review-overview",
                                    div { h2 { "变更审阅" span { "{git_status.read().as_ref().map(|repo| repo.files.len()).unwrap_or(0)}" } } p { "查看当前项目的真实 Git 工作区变化" } }
                                    span { class: "review-progress", "{reviewed_count}/{git_status.read().as_ref().map(|repo| repo.files.len()).unwrap_or(0)} 已审阅" }
                                }
                                div { class: "review-scope",
                                    button { class: "active", "全部" span { class: "tab-count", "{git_status.read().as_ref().map(|repo| repo.files.len()).unwrap_or(0)}" } }
                                    span { "审阅标记仅保存在当前浏览器。" }
                                }
                                div { class: "change-layout",
                                    aside { class: "change-browser",
                                        div { class: "change-filters",
                                            div { class: "change-search", Icon { name: "search".to_owned() } input { aria_label: "搜索变更文件", placeholder: "搜索文件…", value: "{change_query}", oninput: move |event| change_query.set(event.value()) } }
                                            div { class: "change-filter-row",
                                                input { aria_label: "筛选目录", placeholder: "目录…", value: "{change_directory_filter}", oninput: move |event| change_directory_filter.set(event.value()) }
                                                select { aria_label: "筛选变更类型", value: "{change_status_filter}", onchange: move |event| change_status_filter.set(event.value()),
                                                    option { value: "all", "全部类型" }
                                                    option { value: "M", "已修改 M" }
                                                    option { value: "A", "新增 A" }
                                                    option { value: "D", "删除 D" }
                                                    option { value: "R", "重命名 R" }
                                                }
                                                label { input { r#type: "checkbox", checked: *changes_unreviewed.read(), onchange: move |event| changes_unreviewed.set(event.checked()) } "未审阅" }
                                            }
                                        }
                                        div { class: "change-list-caption", "{visible_changes.len()} 个文件" }
                                        if git_status.read().is_none() {
                                            div { class: "change-no-results", "连接项目后加载 Git 状态。" }
                                        } else if visible_changes.is_empty() {
                                            div { class: "change-no-results", strong { "没有匹配的变更" } p { "试试其他路径、类型或审阅筛选。" } }
                                        } else {
                                            div { class: "change-file-list",
                                                for changed in visible_changes.iter() {
                                                    {
                                                        let path = changed.path.clone();
                                                        let status_code = change_status_label(&changed.status);
                                                        let selected = selected_change_file.read().as_ref() == Some(&path);
                                                        let reviewed = reviewed_changes.read().contains(&path);
                                                        rsx! {
                                                            button {
                                                                key: "review-file-{path}",
                                                                class: if selected { "change-file selected" } else { "change-file" },
                                                                onclick: move |_| open_change(path.clone()),
                                                                Icon { name: if changed.status == "D" { "file".to_owned() } else { "file".to_owned() } }
                                                                span { class: "change-file-name", "{changed.path}" small { if changed.staged { "已暂存" } else { "工作区文件" } } }
                                                                span { class: "file-change-status status-{status_code}", "{status_code}" }
                                                                span { class: if reviewed { "file-review-mark reviewed" } else { "file-review-mark" }, aria_label: if reviewed { "已审阅" } else { "未审阅" }, if reviewed { "✓" } else { "·" } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    section { class: "change-preview",
                                        if let Some(path) = selected_change_file.read().clone() {
                                            {
                                                let reviewed_path = path.clone();
                                                rsx! {
                                                    div { class: "selected-file-heading", button { class: "back-to-files", onclick: move |_| change_mobile_preview.set(false), Icon { name: "back".to_owned() } "返回文件列表" } strong { "{path}" } }
                                                    div { class: "review-file-toolbar",
                                                        label { input { r#type: "checkbox", checked: reviewed_snapshot.contains(&reviewed_path), onchange: move |event| {
                                                            if event.checked() { reviewed_changes.write().insert(reviewed_path.clone()); } else { reviewed_changes.write().remove(&reviewed_path); }
                                                        } } "已审阅（仅本地）" }
                                                    }
                                                }
                                            }
                                            if let Some(patch) = change_diff.read().as_ref() {
                                                div { class: "selected-diff-scroll", {render_diff(&patch.patch)} }
                                            } else if let Some(error) = change_error.read().as_ref() {
                                                div { class: "change-preview-empty error-text", strong { "Git patch 读取失败" } p { "{error}" } }
                                            } else {
                                                div { class: "change-preview-empty", "正在加载 Git patch…" }
                                            }
                                        } else {
                                            div { class: "change-preview-empty", "从左侧选择一个文件查看 diff" }
                                        }
                                    }
                                }
                            }
                        }
                        if active_page.read().as_str() == "workspace" {
                            if let Some(session_id) = selected_session.read().clone() {
                                QueueSync { key: "queue-sync-{session_generation}", session_id, client: shared_client, session_generation, queue }
                            }
                        }
                        if active_view.read().as_str() == "conversation" {
                            if let Some(session_id) = selected_session.read().clone() {
                                scroll_follow::ScrollFollow { scope: session_scope_key(&node_url.read(), &session_id), selector: ".transcript".to_owned() }
                            }
                            div { class: "transcript", "data-session-scope": selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                if pending_dialogs.read().is_empty() && transcript.read().is_empty() && projection.read().is_none() {
                                    div { class: "empty-state", strong { "开始一个真实任务" } p { if device_mode.read().is_mobile() { "连接我的电脑，继续项目与会话。" } else { "使用本机执行节点，从项目侧栏打开或新建会话。也可以连接其他节点。" } }
                                        button { class: "button primary", onclick: move |_| { active_page.set("settings".to_owned()); settings_section.set("devices".to_owned()); }, if device_mode.read().is_mobile() { "连接我的电脑" } else { "节点设置" } }
                                    }
                                } else {
                                    for (index, message) in transcript.read().iter().enumerate() {
                                        div { key: "history-{history_id(message, index)}", {conversation::render_history(message)} }
                                    }
                                    if let Some(run) = projection.read().as_ref() {
                                        for block in run.blocks.iter().filter(|block| !history_has_block(&transcript.read(), block)) {
                                            div { key: "block-{block.id()}", {conversation::render_message_block(block)} }
                                        }
                                        p { class: "timeline-phase", "运行阶段：{phase_label(&run.phase)} · {connection_label(&run.connection)}" }
                                    }
                                    for pending in pending_dialogs.read().iter().cloned() {
                                        { let request_id = pending.request_id.clone(); let scope = session_scope_key(&node_url.read(), selected_session.read().as_deref().unwrap_or_default()); let key = format!("{scope}::{request_id}"); rsx! {
                                            conversation::AskCard {
                                                key: "ask-{key}", pending,
                                                submitting: dialog_busy.read().contains(&key),
                                                error: dialog_errors.read().get(&key).cloned(),
                                                onanswer: answer_dialog,
                                            }
                                        } }
                                    }
                                }
                            }
                            message_queue::QueueShelf { snapshot: queue.read().clone(), onopen: move |()| queue_open.set(true) }
                        div { class: "composer-main",
                            div { class: if running { "composer is-running" } else { "composer" },
                            if let Some(session_id) = selected_session.read().clone() {
                                { let scope = session_scope_key(&node_url.read(), &session_id); rsx! { composer_extras::AttachmentPicker { scope: scope.clone(), store: attachment_store, presentation: "files".to_owned(), onquote: move |text| {
                                    if let Some(id) = selected_session.read().as_deref() { let draft_scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&draft_scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &draft_scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); }
                                } } } }
                            }
                            textarea {
                                aria_label: "发送给 agent 的消息",
                                rows: 2,
                                value: "{composer}",
                                placeholder: "继续描述你的想法，或输入 / 使用命令…",
                                oninput: move |event| if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); update_draft(drafts, &scope, event.value()); },
                                onkeydown: move |event| {
                                    if event.key() == Key::Enter && !event.modifiers().shift() && !event.is_composing() {
                                        event.prevent_default();
                                        if let Some(id) = selected_session.read().clone() {
                                            let scope = session_scope_key(&node_url.read(), &id);
                                            if sending.read().contains(&scope) {
                                                let draft = state::draft_for(&drafts.read(), &scope);
                                                if !draft.text.trim().is_empty() {
                                                    pending_enter.set(Some((scope, draft.revision, *session_generation.read())));
                                                }
                                            } else { send_or_command(); }
                                        }
                                    }
                                }
                            }
                            div { class: "composer-capabilities",
                                button { class: "button", onclick: move |_| { if selected_session.read().is_some() { btw_open.set(true); } else { status.set("请先打开主会话，再创建独立旁问。".into()); } }, "BTW" }
                                details { class: "capability-menu",
                                summary { "能力" }
                                div { class: "capability-menu-items",
                                message_queue::QueueEntry { snapshot: queue.read().clone(), onopen: move |()| queue_open.set(true) }
                                button { class: "button", onclick: move |_| session_details_open.set(true), "Tools" }
                                button { class: "button", onclick: move |_| status.set("当前为节点 Agent 会话；可在 Tools 中配置真实工具。节点未提供 Plan/Ask 模式切换协议。".into()), "Agent 模式" }
                                button { class: "button", onclick: move |_| status.set("可用命令：/btw [问题]、/compact、/new、/help。".into()), "/ 命令" }
                                button { class: "button", disabled: selected_session.read().is_none(), onclick: move |_| context_open.set(true), "压缩上下文" }
                                }
                                }
                                if let Some(session_id) = selected_session.read().clone() {
                                    resources::SkillPicker { session_id: session_scope_key(&node_url.read(), &session_id), library: resource_library, open_session_config, onquote: move |text: String| {
                                        if let Some(id) = selected_session.read().as_deref() {
                                            let scope = session_scope_key(&node_url.read(), id);
                                            let existing = drafts.read().get(&scope).map(|draft| draft.text.clone()).unwrap_or_default();
                                            let text = if existing.is_empty() { text } else { format!("{existing}\n{text}") };
                                            update_draft(drafts, &scope, text);
                                        }
                                    } }
                                }
                            }
                            div { class: "composer-bottom",
                                div { class: "composer-options",
                                    message_queue::DeliveryPicker { value: queue_delivery.read().clone(), onchange: move |value| queue_delivery.set(value) }
                                    if let Some(session_id) = selected_session.read().clone() {
                                        { let scope = session_scope_key(&node_url.read(), &session_id); rsx! { composer_extras::AttachmentPicker { scope, store: attachment_store, presentation: "button".to_owned(), onquote: move |text| {
                                            if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); }
                                        } } } }
                                    }
                                    span { class: "model-pill", "D" }
                                    button { class: "composer-option", onclick: move |_| session_details_open.set(true), "{runtime_state.read().as_ref().and_then(|r| r.model_id.as_deref()).unwrap_or(\"模型 · —\")}" }
                                    span { class: "option-divider" }
                                    button { class: "composer-option", onclick: move |_| session_details_open.set(true), Icon { name: "sparkles".to_owned() } "{runtime_state.read().as_ref().and_then(|r| r.thinking_level.as_deref()).unwrap_or(\"思考 · —\")}" }
                                }
                                div { class: "composer-actions",
                                    if running {
                                        button { class: "secondary", aria_label: "停止运行", onclick: abort, "停止" }
                                        button { class: "send-button", disabled: submitting, aria_label: "按所选方式投递消息", onclick: move |_| send_or_command(), Icon { name: "arrow".to_owned() } }
                                    } else {
                                        button { class: "send-button", disabled: selected_session.read().is_none(), aria_label: if running { "加入消息队列" } else { "发送消息" }, onclick: move |_| send_or_command(), Icon { name: "arrow".to_owned() } }
                                    }
                                }
                            }
                            }
                            div { class: "composer-foot",
                                span { role: "status", span { class: "status-dot" } "{status}" }
                                if let Some(session_id) = selected_session.read().as_ref() {
                                    traces::TraceEntry { scope: session_scope_key(&node_url.read(), session_id), store: traces }
                                }
                                span { "Enter 发送 · Shift + Enter 换行" }
                            }
                            runtime_stats::RuntimeFooter {
                                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                runtime_state, projection, transcript, stats_open, context_open,
                                ontools: move |()| session_details_open.set(true),
                            }
                        }
                        }
                    }
                }
                }
                aside { class: if *inspector_open.read() { "inspector visible" } else { "inspector" },
                        h3 { "任务上下文" button { class: "inspector-close", aria_label: "关闭任务上下文", onclick: move |_| inspector_open.set(false), Icon { name: "close".to_owned() } } }
                        div { class: "inspector-content",
                        if let Some((project, resource, association)) = meta_inspector.clone() {
                            section { class: "inspector-card meta-inspector", id: "meta-agent-context", aria_label: "元Agent上下文",
                                div { class: "meta-inspector-label", "元Agent" span { if association.is_some() { "开发会话" } else { "能力项目" } } }
                                if let Some(resource) = resource.as_ref() {
                                    strong { class: "view-title", "{resource.name}" }
                                    if let Some(association) = association.as_ref() {
                                        small { "{resource.kind} · {association}" }
                                        small { "关联快照{association.rsplit_once('#').map_or(\"未报告\", |(_, revision)| revision)} · 当前源稿 r{resource.revision}" }
                                    } else {
                                        small { "{resource.kind} · {resource.version} · 当前草稿 r{resource.revision}" }
                                    }
                                } else {
                                    strong { class: "view-title", "关联源稿未找到" }
                                    if let Some(association) = association.as_ref() { small { "{association} · 关联快照，源稿未报告" } }
                                    small { "可返回能力项目核对资源；不会使用其他项目的同名能力。" }
                                }
                                div { class: "data-line", span { "验证范围" } span { "本地结构检查" } }
                                button { class: "small-link", onclick: move |_| {
                                    let same_project = selected_project.read().as_deref() == Some(project.as_str());
                                    if same_project {
                                        if let Some(resource) = resource.as_ref() {
                                            meta_store.write().selected_by_project.insert(project.clone(), resource.id.clone());
                                            active_page.set("meta-agent".into());
                                        } else {
                                            status.set("当前项目中找不到关联源稿；请从元Agent导航重新选择能力项目。".into());
                                        }
                                    } else {
                                        status.set("关联能力属于其他项目；请从项目切换入口打开，避免切换会话时丢失项目上下文。".into());
                                    }
                                }, "返回能力项目" Icon { name: "chevron-right" } }
                            }
                        }
                        div { class: "inspector-card overview-card runtime-context",
                            strong { class: "inspector-label", "运行概览" }
                            small { class: "inspector-run-status", "{projection_status(projection.read().as_ref(), status.read().as_str())}" }
                            button { class: "inspector-context-button", aria_label: "管理当前上下文", onclick: move |_| context_open.set(true),
                                span { "上下文" strong { "{inspector_context}" } }
                                span { "{runtime_stats::percent(inspector_context_percent)}" Icon { name: "chevron-right".to_owned() } }
                            }
                            div { class: "context-track", aria_label: "上下文占比 {runtime_stats::percent(inspector_context_percent)}",
                                span { style: "width:{inspector_context_percent.unwrap_or(0.0).clamp(0.0, 100.0)}%" }
                                if let Some(threshold) = inspector_threshold {
                                    i { style: "left:{threshold.clamp(0.0, 100.0)}%", title: "节点报告阈值 {runtime_stats::percent(Some(threshold))}" }
                                }
                            }
                            div { class: "data-line", span { "Provider" } span { title: "{inspector_provider}", "{inspector_provider}" } }
                            div { class: "data-line", span { "模型" } span { title: "{inspector_model}", "{inspector_model}" } }
                            div { class: "data-line", span { "思考强度" } span { "{inspector_thinking}" } }
                            div { class: "data-line", span { "节点报告阈值" } span { "{runtime_stats::percent(inspector_threshold)}" } }
                            div { class: "runtime-shortcuts",
                                button { onclick: move |_| { active_page.set("settings".into()); settings_section.set("providers".into()); }, Icon { name: "server".to_owned() } "Provider" }
                                button { onclick: move |_| context_open.set(true), Icon { name: "panel".to_owned() } "管理上下文" }
                                button { onclick: move |_| queue_open.set(true), Icon { name: "message".to_owned() } "队列 {queue.read().as_ref().map(|q| q.count).unwrap_or(0)}" }
                            }
                        }
                        div { class: "inspector-card",
                            strong { "执行环境" }
                            small { "rpi-http · {node_url}" }
                            small { "{selected_project_path.read().as_deref().unwrap_or(\"未选择项目\")}" }
                            div { class: "runtime-shortcuts",
                                button { onclick: move |_| { active_page.set("settings".into()); settings_section.set("devices".into()); }, Icon { name: "settings".to_owned() } if device_mode.read().is_mobile() { "连接我的电脑" } else { "节点设置" } }
                            }
                        }
                        div { class: "inspector-card",
                            strong { "计划" }
                            if inspector_plan.is_empty() {
                                small { "尚无任务计划" }
                            } else {
                                small { "{inspector_plan.iter().filter(|item| item.status == \"completed\").count()}/{inspector_plan.len()} 已完成" }
                                div { class: "inspector-plan",
                                    for item in inspector_plan.iter() {
                                        div { class: "inspector-plan-item {item.status}",
                                            span { class: "plan-status-icon", title: "{item.label}",
                                                if item.status == "completed" { Icon { name: "check".to_owned() } }
                                                else if item.status == "running" { Icon { name: "refresh".to_owned() } }
                                                else { span { class: "plan-pending-dot" } }
                                            }
                                            div { span { class: "plan-title", "{item.title}" } small { "{item.label}" } }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(caps) = capabilities.read().as_ref() {
                            details { class: "inspector-card diagnostic-details",
                                summary { "诊断详情" }
                                small { "协议命令：{caps.supported_commands.len()} · 动作：{caps.supported_actions.len()}" }
                                if let Some(run) = projection.read().as_ref() { small { "原始游标：{run.cursor.unwrap_or_default()} · 事件块：{run.blocks.len()}" } }
                            }
                        }
                        if let Some(repository) = git_status.read().as_ref() {
                            div { class: "inspector-card",
                                strong { "本轮文件" }
                                small { "工作区变更 · 共 {repository.files.len()} 个文件" }
                                div { class: "inspector-files",
                                    for file in repository.files.iter().take(5) {
                                        div { class: "inspector-file-row", title: "{file.path}",
                                            Icon { name: "file".to_owned() }
                                            span { class: "inspector-file-path", "{file.path}" }
                                            span { class: "inspector-file-status", "{file.status}" }
                                        }
                                    }
                                }
                                button { class: "inspector-small-button", onclick: move |_| { active_page.set("workspace".into()); active_view.set("changes".into()); }, "查看全部变更" }
                            }
                        }
                        if sessions.read().iter().find(|item| selected_session.read().as_deref() == Some(item.session_id.as_str())).is_some_and(|item| item.pending_resume) {
                            div { class: "inspector-card resume-pending",
                                strong { "会话等待恢复" }
                                small { "节点报告 pendingResume；不会自动重发消息。" }
                                button { class: "button", onclick: move |_| if let (Some(id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) { state::scoped_spawn(requests, async move { match client.resume_pending(&id).await { Ok(_) => status.set("恢复请求已提交，等待节点结算事件。".into()), Err(error) => status.set(format!("恢复失败：{error}")) } }); }, "恢复会话" }
                            }
                        }
                        }
                        div { class: "inspector-footer", span { class: "tiny-logomark", "r↗" } div { span { "你的工具。你的模型。" } span { "你的工作方式。" } } }
                }
                if *appearance_open.read() {
                    AppearancePicker { settings: appearance, onclose: move |()| appearance_open.set(false) }
                }
                if *session_details_open.read() {
                    Modal { id: "session-details", title: "会话详情与配置", class: "session-details-modal", onclose: move |()| { session_details_open.set(false); delete_confirmation.set(false); },
                        div { class: "modal-body",
                            p { "{selected_session_title.read().as_deref().unwrap_or(\"未选择会话\")}" }
                            if let Some(runtime) = runtime_state.read().as_ref() {
                                dl { class: "runtime-list",
                                    div { dt { "Provider" } dd { "{runtime.provider.as_deref().unwrap_or(\"节点未返回\")}" } }
                                    div { dt { "模型" } dd { "{runtime.model_id.as_deref().unwrap_or(\"节点未返回\")}" } }
                                    div { dt { "工具" } dd { "{runtime.active_tools.join(\", \")}" } }
                                }
                            }
                            session_config::SessionConfig { shared_client, selected_session, session_generation, requests, runtime_state, status, tools_only: false, oncompact: move |()| { session_details_open.set(false); context_open.set(true); } }
                            div { class: "form-field", label { r#for: "session-title", "会话标题（重命名 / Fork / Clone）" }
                                input { id: "session-title", value: "{session_title_draft}", oninput: move |event| session_title_draft.set(event.value()) }
                            }
                            div { class: "session-actions",
                                button { disabled: selected_session.read().is_none(), onclick: rename_session, "重命名" }
                                button { disabled: selected_session.read().is_none(), onclick: fork_session, "Fork" }
                                button { disabled: selected_session.read().is_none(), onclick: clone_session, "Clone" }
                                button { disabled: selected_session.read().is_none(), onclick: move |_| delete_confirmation.set(true), "删除" }
                            }
                            if *delete_confirmation.read() {
                                div { class: "dialog", strong { "确认删除会话？" } p { "此操作会删除服务器上的会话历史，且无法撤销。" }
                                    div { class: "dialog-actions", button { onclick: delete_session, "确认删除" } button { onclick: move |_| delete_confirmation.set(false), "取消" } }
                                }
                            }
                            p { role: "status", class: "meta", "{status}" }
                        }
                    }
                }
                if *project_files_open.read() {
                    Modal { id: "project-files", title: "项目文件", subtitle: format!("{} · 实时文件 · 只读", selected_project_name.read().as_deref().unwrap_or("项目")), class: if *project_file_preview.read() { "project-files-modal show-preview" } else { "project-files-modal" }, onclose: move |()| { project_files_open.set(false); project_file_preview.set(false); },
                            div { class: "project-files-layout",
                                if let (Some(_client), Some(project)) = (shared_client.read().clone(), selected_project.read().clone()) {
                                    project_files::Browser { key: "{node_url}-{project}", client: shared_client, project: project.clone(), scope: format!("{node_url}\u{1f}{project}"),
                                        selected: selected_project_file.read().clone(),
                                        onselect: open_file,
                                        oninitial: move |path| { if selected_project_file.peek().is_none() { open_file(path); project_file_preview.set(false); } },
                                        onreported: { let scope = format!("{node_url}\u{1f}{project}"); move |entries| reported_files.set((scope.clone(), entries)) },
                                    }
                                } else { aside { class: "project-file-browser", p { class: "project-no-results", "选择项目并连接节点后加载文件。" } } }
                                section { class: "project-file-preview",
                                    button { class: "project-back", onclick: move |_| project_file_preview.set(false), "返回文件树" }
                                    if *file_loading.read() {
                                        div { class: "project-preview-empty", "正在加载文件内容…" }
                                    } else if let Some(error) = file_error.read().as_ref() {
                                        div { class: "project-preview-empty error-text", strong { "文件内容读取失败" } p { "{error}" } button { onclick: move |_| { if let Some(path) = selected_project_file.read().clone() { open_file(path); } }, "重试读取文件" } }
                                    } else if let Some(content) = file_content.read().as_ref() {
                                        div { class: "project-preview-heading",
                                            div { class: "project-preview-title", strong { "{content.path.rsplit('/').next().unwrap_or(&content.path)}" } small { "{content.path}" } }
                                        }
                                        div { class: "project-preview-tools",
                                            span { class: "project-file-type", "{project_file_kind(&content.path)} · 只读" }
                                            if reported_files.read().0 == format!("{}\u{1f}{}", node_url, selected_project.read().as_deref().unwrap_or_default()) {
                                                if let Some(entry) = reported_files.read().1.iter().find(|entry| entry.path == content.path) {
                                                    span { class: "project-file-size", "{entry.size} B · 节点报告" }
                                                }
                                            }
                                            button { class: if project_file_mode.read().as_str() == "preview" { "active" } else { "" }, disabled: project_file_kind(&content.path) == "code", onclick: move |_| project_file_mode.set("preview".into()), "预览" }
                                            button { class: if project_file_mode.read().as_str() == "source" { "active" } else { "" }, onclick: move |_| project_file_mode.set("source".into()), "源码" }
                                            button { onclick: move |_| { let wrap = *project_file_wrap.read(); project_file_wrap.set(!wrap); }, if *project_file_wrap.read() { "关闭换行" } else { "自动换行" } }
                                            button { class: "project-quote", onclick: { let path = content.path.clone(); move |_| quote_project_file(path.clone()) }, "引用路径到草稿" }
                                        }
                                        div { class: "project-preview-content", {render_project_file(content, project_file_mode.read().as_str(), *project_file_wrap.read())} }
                                    } else {
                                        div { class: "project-preview-empty", "从左侧选择文件查看内容" }
                                    }
                                }
                            }
                    }
                }
                if *global_search_open.read() {
                    project_picker::SearchDialog {
                        projects: projects.read().to_vec(), sessions: sessions.read().to_vec(),
                        onproject: move |id| { global_search_open.set(false); open_project(id); },
                        onsession: move |id| { global_search_open.set(false); open_session(id); },
                        onclose: move |()| global_search_open.set(false),
                    }
                }
                if *session_switcher_open.read() {
                    project_picker::SessionSwitcher {
                        projects: projects.read().to_vec(), sessions: sessions.read().to_vec(),
                        current_session: selected_session.read().clone(),
                        onsession: move |id| { session_switcher_open.set(false); open_session(id); },
                        onclose: move |()| session_switcher_open.set(false),
                    }
                }
            if *btw_open.read() && active_page.read().as_str() == "workspace" {
                if let Some(session_id) = selected_session.read().clone() {
                    BtwHost { onclose: move |()| btw_open.set(false),
                        composer_extras::BtwPanel {
                            scope: session_scope_key(&node_url.read(), &session_id), project_id: selected_project.read().clone(), client: shared_client, store: btw_store,
                            onquote: move |text| { if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); } },
                            onclose: move |()| btw_open.set(false),
                        }
                    }
                }
            }
        }
    }
}

/// One request at a time; visibility and scope lifetime own the timer.
#[component]
fn BtwHost(onclose: EventHandler, children: Element) -> Element {
    rsx! {
        dialog {
            id: "btw-overlay",
            class: "app-dialog btw-overlay",
            aria_label: "BTW 独立旁问",
            onmounted: move |_| {
                spawn(async move {
                    let script = r"
                        const dialog = document.getElementById('btw-overlay');
                        if (!dialog) return;
                        const media = matchMedia('(max-width: 1040px)');
                        let ignoredCloses = 0;
                        const sync = () => {
                            if (!dialog.isConnected) return;
                            if (dialog.open) { ignoredCloses++; dialog.close(); }
                            if (media.matches) dialog.showModal(); else dialog.show();
                        };
                        const closed = () => {
                            if (ignoredCloses) { ignoredCloses--; return; }
                            dioxus.send(true);
                        };
                        dialog.addEventListener('close', closed);
                        media.addEventListener('change', sync);
                        sync();
                        const observer = new MutationObserver(() => {
                            if (!dialog.isConnected) {
                                observer.disconnect();
                                media.removeEventListener('change', sync);
                                dialog.removeEventListener('close', closed);
                                dioxus.send(false);
                            }
                        });
                        observer.observe(document.body, {childList:true, subtree:true});
                    ";
                    if let Ok(true) = document::eval(script).recv::<bool>().await { onclose.call(()); }
                });
            },
            button { class: "btw-scrim", r#type: "button", aria_label: "关闭旁问遮罩", onclick: move |_| onclose.call(()) }
            {children}
        }
    }
}

#[component]
fn QueueSync(
    session_id: String,
    client: Signal<Option<WorkClient>>,
    session_generation: Signal<u64>,
    mut queue: Signal<Option<QueueSnapshot>>,
) -> Element {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = use_hook(|| NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut task = use_signal(|| None::<Task>);
    use_effect(move || {
        let Some(node) = client.read().clone() else {
            return;
        };
        let generation = *session_generation.peek();
        let session = session_id.clone();
        if let Some(previous) = task.write().take() {
            previous.cancel();
        }
        task.set(Some(spawn(async move {
            let script = format!(r"
                window.__rpiQueueSync ??= new Map();
                window.__rpiQueueSync.get({id})?.dispose();
                let timer = null, pending = false, alive = true;
                const tick = () => {{
                    if (!alive || document.hidden || pending) return;
                    pending = true;
                    dioxus.send(true);
                }};
                const changed = () => {{ clearTimeout(timer); timer = null; if (!document.hidden) tick(); }};
                const state = {{
                    complete() {{
                        pending = false;
                        if (alive && !document.hidden) timer = setTimeout(tick, 3000);
                    }},
                    dispose() {{
                        alive = false; clearTimeout(timer);
                        document.removeEventListener('visibilitychange', changed);
                        if (window.__rpiQueueSync.get({id}) === state) window.__rpiQueueSync.delete({id});
                    }}
                }};
                window.__rpiQueueSync.set({id}, state);
                document.addEventListener('visibilitychange', changed);
                tick();
            ");
            let mut clock = document::eval(&script);
            while clock.recv::<bool>().await.is_ok() {
                let snapshot = node.queue(&session, false).await;
                if *session_generation.peek() != generation { break; }
                if let Ok(snapshot) = snapshot { queue.set(Some(snapshot)); }
                let _ = document::eval(&format!("window.__rpiQueueSync?.get({id})?.complete();"));
            }
        })));
    });
    use_drop(move || {
        let _ = document::eval(&format!("window.__rpiQueueSync?.get({id})?.dispose();"));
        if let Some(task) = task.write().take() {
            task.cancel();
        }
    });
    rsx! {}
}

fn projection_status(projection: Option<&RunProjection>, fallback: &str) -> &'static str {
    if let Some(projection) = projection {
        phase_label(&projection.phase)
    } else if fallback.contains("失败") || fallback.contains("错误") {
        "连接错误"
    } else if fallback.contains("在线") {
        "在线"
    } else {
        "等待连接"
    }
}

fn project_file_kind(path: &str) -> &'static str {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown") {
        "markdown"
    } else if extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm") {
        "html"
    } else if ["png", "jpg", "jpeg", "gif", "webp"]
        .iter()
        .any(|image| extension.eq_ignore_ascii_case(image))
    {
        "image"
    } else {
        "code"
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn markdown_static_html(text: &str) -> String {
    let mut html = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_code {
                html.push_str("</code></pre>");
            } else {
                html.push_str("<pre><code>");
            }
            in_code = !in_code;
        } else if in_code {
            html.push_str(&escape_html(line));
            html.push('\n');
        } else if let Some(heading) = line.strip_prefix("# ") {
            html.push_str("<h1>");
            html.push_str(&escape_html(heading));
            html.push_str("</h1>");
        } else if let Some(heading) = line.strip_prefix("## ") {
            html.push_str("<h2>");
            html.push_str(&escape_html(heading));
            html.push_str("</h2>");
        } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            html.push_str("<p>• ");
            html.push_str(&escape_html(item));
            html.push_str("</p>");
        } else if !line.trim().is_empty() {
            html.push_str("<p>");
            html.push_str(&escape_html(line));
            html.push_str("</p>");
        }
    }
    if in_code {
        html.push_str("</code></pre>");
    }
    html
}

fn static_html_preview(source: &str) -> String {
    let mut rendered = String::new();
    let mut inside_title = false;
    for fragment in source.split('<') {
        let Some((tag, text)) = fragment.split_once('>') else {
            rendered.push_str(&escape_html(fragment));
            continue;
        };
        let tag = tag.trim().to_ascii_lowercase();
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if name == "title" {
            inside_title = !closing;
            continue;
        }
        if inside_title {
            continue;
        }
        match (name, closing) {
            ("h1" | "h2" | "h3", false) => rendered.push_str("<h1>"),
            ("h1" | "h2" | "h3", true) => rendered.push_str("</h1>"),
            ("p", false) => rendered.push_str("<p>"),
            ("p", true) => rendered.push_str("</p>"),
            ("a", false) => rendered.push_str("<span>"),
            ("a", true) => rendered.push_str("</span>"),
            _ => {}
        }
        rendered.push_str(&escape_html(text));
    }
    rendered
}

fn render_project_file(content: &work_client::FileContent, mode: &str, wrap: bool) -> Element {
    let kind = project_file_kind(&content.path);
    if mode == "preview" && kind == "markdown" {
        return rsx! { article { class: "project-markdown", dangerous_inner_html: markdown_static_html(&content.content) } };
    }
    if mode == "preview" && kind == "html" {
        let srcdoc = format!(
            "<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; form-action 'none'; base-uri 'none'\"><style>body{{font:13px/1.6 system-ui;padding:16px;color:#20252b}}h1{{font-size:22px}}a{{color:#176b87}}</style><body>{}</body>",
            static_html_preview(&content.content)
        );
        let iframe = format!(
            "<iframe class=\"project-html-preview\" sandbox=\"allow-same-origin\" referrerpolicy=\"no-referrer\" title=\"HTML 文件静态预览\" srcdoc=\"{}\"></iframe>",
            escape_html(&srcdoc)
        );
        return rsx! {
            div { class: "project-html-note", "静态 HTML · 禁止脚本、外部资源与外站导航" }
            div { class: "project-html-frame", dangerous_inner_html: iframe }
        };
    }
    let lines = content
        .content
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.to_owned()))
        .collect::<Vec<_>>();
    rsx! {
        pre { class: if wrap { "project-code wrap-lines" } else { "project-code" },
            for (number, line) in lines {
                span { class: "project-code-line", i { "{number}" } code { "{line}" } }
            }
        }
    }
}

fn resource_snapshot(
    binding: &resources::SessionBinding,
    library: &resources::ResourceLibrary,
) -> Value {
    let packages = binding.package_pins.iter().map(|pin| {
        library.packages.iter().find(|item| item.id == pin.id && item.version == pin.version).map_or_else(
            || json!({"id":pin.id,"version":pin.version,"unresolved":true}),
            |item| json!({"id":item.id,"version":item.version,"manifest":item.manifest,"skills":item.skill_contents.iter().filter(|skill| !binding.disabled_skills.contains(&format!("{}@{}/{}",item.id,item.version,skill.id))).map(|skill| json!({"id":format!("{}@{}/{}",item.id,item.version,skill.id),"name":skill.name,"description":skill.description,"content":skill.content})).collect::<Vec<_>>(),"tools":item.tools,"prompts":item.prompts.iter().map(|prompt| json!({"id":format!("{}@{}/{}",item.id,item.version,prompt.id),"name":prompt.name,"kind":prompt.kind,"content":prompt.content})).collect::<Vec<_>>()})
        )
    }).collect::<Vec<_>>();
    let skills = resources::effective_skills(library, binding)
        .iter()
        .map(|skill| json!({"id":skill.id,"revision":skill.content.len(),"content":skill.content}))
        .collect::<Vec<_>>();
    json!({
        "desiredRevision":binding.revision,
        "packages":packages,
        "skills":skills,
        "disabledSkills":binding.disabled_skills,
        "disabledTools":binding.disabled_tools,
        "disabledPrompts":binding.disabled_prompts,
        "instructions":binding.instructions
    })
}

fn node_origin(url: &str) -> String {
    let trimmed = url.trim();
    let Some((scheme, remainder)) = trimmed.split_once("://") else {
        return trimmed.to_ascii_lowercase();
    };
    let scheme = scheme.to_ascii_lowercase();
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let (host, port) = if let Some(end) = authority.find(']') {
        (&authority[..=end], authority[end + 1..].strip_prefix(':'))
    } else {
        authority
            .rsplit_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)))
    };
    let port = port.and_then(|port| port.parse::<u16>().ok());
    let port = port
        .filter(|port| !((scheme == "http" && *port == 80) || (scheme == "https" && *port == 443)));
    let host = host.to_ascii_lowercase();
    match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

fn project_scope_key(origin: &str, project_id: &str) -> String {
    format!("{}::project::{project_id}", node_origin(origin))
}

fn session_scope_key(origin: &str, session_id: &str) -> String {
    format!("{}::session::{session_id}", node_origin(origin))
}

fn change_status_label(status: &str) -> &'static str {
    match status.to_ascii_uppercase().as_str() {
        "A" | "ADDED" | "ADD" => "A",
        "D" | "DELETED" | "DELETE" => "D",
        "R" | "RENAMED" | "RENAME" => "R",
        _ => "M",
    }
}

fn filter_changed_files<'a>(
    files: &'a [work_client::ChangedFile],
    query: &str,
    status: &str,
    directory: &str,
    unreviewed: bool,
    reviewed: &std::collections::HashSet<String>,
) -> Vec<&'a work_client::ChangedFile> {
    let query = query.trim().to_ascii_lowercase();
    files
        .iter()
        .filter(|file| {
            let matches_query = query.is_empty()
                || file.path.to_ascii_lowercase().contains(&query)
                || file
                    .original_path
                    .as_deref()
                    .is_some_and(|old| old.to_ascii_lowercase().contains(&query));
            let matches_status = status == "all" || change_status_label(&file.status) == status;
            let matches_directory =
                directory.trim().is_empty() || file.path.starts_with(directory.trim_matches('/'));
            matches_query
                && matches_status
                && matches_directory
                && (!unreviewed || !reviewed.contains(&file.path))
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedDiffLine {
    kind: &'static str,
    old: Option<u32>,
    new: Option<u32>,
    text: String,
}

fn parse_diff_range(value: &str) -> Option<u32> {
    value
        .split_once(',')
        .map_or(value, |(start, _)| start)
        .trim()
        .parse()
        .ok()
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let mut fields = line.split_whitespace();
    fields.next()?;
    let old = fields.next()?.strip_prefix('-')?;
    let new = fields.next()?.strip_prefix('+')?;
    Some((parse_diff_range(old)?, parse_diff_range(new)?))
}

fn parse_unified_diff(patch: &str) -> Vec<ParsedDiffLine> {
    let mut old_line = None;
    let mut new_line = None;
    let mut parsed = Vec::new();
    for line in patch.lines() {
        if line.starts_with("@@") {
            if let Some((old, new)) = parse_hunk_header(line) {
                old_line = Some(old);
                new_line = Some(new);
                parsed.push(ParsedDiffLine {
                    kind: "hunk",
                    old: None,
                    new: None,
                    text: line.to_owned(),
                });
            }
            continue;
        }
        if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            continue;
        }
        if line.starts_with("\\ No newline") {
            parsed.push(ParsedDiffLine {
                kind: "note",
                old: None,
                new: None,
                text: line.to_owned(),
            });
            continue;
        }
        let (kind, old, new, text) = match line.as_bytes().first().copied() {
            Some(b'+') => {
                let new = new_line;
                new_line = new_line.map(|line| line + 1);
                ("add", None, new, line[1..].to_owned())
            }
            Some(b'-') => {
                let old = old_line;
                old_line = old_line.map(|line| line + 1);
                ("del", old, None, line[1..].to_owned())
            }
            Some(b' ') | None => {
                let old = old_line;
                let new = new_line;
                old_line = old_line.map(|line| line + 1);
                new_line = new_line.map(|line| line + 1);
                (
                    "context",
                    old,
                    new,
                    line.get(1..).unwrap_or_default().to_owned(),
                )
            }
            _ => continue,
        };
        parsed.push(ParsedDiffLine {
            kind,
            old,
            new,
            text,
        });
    }
    parsed
}

fn render_diff(patch: &str) -> Element {
    let binary = patch.contains("Binary files") || patch.contains("GIT binary patch");
    let lines = parse_unified_diff(patch);
    if binary {
        return rsx! {
            div { class: "binary-preview",
                Icon { name: "file".to_owned() }
                strong { "二进制文件" }
                p { "Git 返回二进制变更，无法生成文本 diff。" }
            }
        };
    }
    if lines.is_empty() {
        return rsx! {
            div { class: "change-preview-empty",
                strong { if patch.trim().is_empty() { "没有文本 diff" } else { "无法解析此 patch" } }
                p { "该文件可能为空、未追踪，或节点返回了非 unified 格式。" }
            }
        };
    }
    rsx! {
        div { class: "diff-lines",
            for line in lines {
                if line.kind == "hunk" {
                    div { class: "diff-hunk", "{line.text}" }
                } else {
                    {
                        let old_number = line.old.map_or_else(String::new, |number| number.to_string());
                        let new_number = line.new.map_or_else(String::new, |number| number.to_string());
                        let marker = if line.kind == "add" { "+" } else if line.kind == "del" { "−" } else { " " };
                        rsx! {
                            div { class: "diff-line {line.kind}",
                                span { class: "line-number", "{old_number}" }
                                span { class: "line-number", "{new_number}" }
                                span { class: "diff-marker", "{marker}" }
                                span { "{line.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn message_id(message: &Value) -> Option<&str> {
    ["entryId", "messageId", "id"]
        .iter()
        .find_map(|key| message.get(key)?.as_str())
}

fn history_id(message: &Value, index: usize) -> String {
    message_id(message).map_or_else(|| format!("snapshot-{index}"), str::to_owned)
}

fn history_has_block(messages: &[Value], block: &MessageBlock) -> bool {
    messages.iter().any(|message| {
        if let Some(id) = message_id(message) {
            if block.id() == id || block.id().starts_with(&format!("{id}-text-")) {
                return true;
            }
        }
        message.get("toolCallId").and_then(Value::as_str) == Some(block.id())
    })
}

fn default_node_url() -> String {
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    {
        // Web discovers its serving node; Android asks for the computer's
        // address. Phone loopback would point back to the phone itself.
        "about:blank".to_owned()
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        "http://127.0.0.1:3030/".to_owned()
    }
}

fn connection_failure_message(error: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = error;
        "连接失败：请检查节点地址、网络与允许来源；Web 建议从节点提供的页面访问。".into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("连接失败：{error}")
    }
}

fn connection_label(connection: &ConnectionState) -> &'static str {
    match connection {
        ConnectionState::Online => "在线",
        ConnectionState::Reconnecting => "重连中",
        ConnectionState::Resyncing => "恢复快照",
        ConnectionState::Offline => "离线",
    }
}

#[allow(dead_code)]
fn phase_label(phase: &RunPhase) -> &'static str {
    match phase {
        RunPhase::Ready => "就绪",
        RunPhase::Accepted => "已接收",
        RunPhase::Preparing => "准备中",
        RunPhase::Text => "生成文本",
        RunPhase::Tool => "执行工具",
        RunPhase::AwaitingInput => "等待确认",
        RunPhase::AwaitingSettlement => "等待结算",
        RunPhase::Cancelling => "停止中",
        RunPhase::Settled(outcome) => match outcome {
            SettlementOutcome::Completed => "已完成",
            SettlementOutcome::Failed => "执行失败",
            SettlementOutcome::Interrupted => "已中断",
            SettlementOutcome::Suspended => "已挂起",
            SettlementOutcome::Compacted => "已压缩",
            SettlementOutcome::Declined => "未执行",
            SettlementOutcome::Unknown(_) => "已结算（未知结果）",
        },
    }
}

#[allow(dead_code)]
fn session_matches(session: &SessionSummary, query: &str, projects: &[Project]) -> bool {
    let project = projects
        .iter()
        .find(|p| session.project_id.as_deref() == Some(p.id.as_str()));
    format!(
        "{} {} {}",
        session.title.as_deref().unwrap_or("未命名会话"),
        session.status,
        project
            .map(|p| format!("{} {}", p.name, p.path))
            .unwrap_or_default()
    )
    .to_lowercase()
    .contains(&query.trim().to_lowercase())
}

#[cfg(test)]
mod scope_identity_tests {
    use super::{node_origin, project_scope_key, session_scope_key};

    #[test]
    fn origins_ignore_paths_credentials_case_and_default_ports() {
        assert_eq!(
            node_origin(" HTTP://Example.COM:080/base?query#fragment "),
            "http://example.com"
        );
        assert_eq!(
            node_origin("https://user:secret@Example.COM:443?query"),
            "https://example.com"
        );
        assert_eq!(node_origin("http://[::1]:80/base"), "http://[::1]");
        assert_eq!(node_origin("http://[::1]:3030/"), "http://[::1]:3030");
    }

    #[test]
    fn project_and_session_identity_preserve_distinct_nodes() {
        let first = "http://localhost:3030/";
        let second = "http://localhost:4040/";
        let mut purposes = std::collections::HashMap::new();
        purposes.insert(project_scope_key(first, "same-id"), "元Agent");
        purposes.insert(project_scope_key(second, "same-id"), "普通项目");
        assert_eq!(purposes.len(), 2);
        assert_eq!(
            purposes[&project_scope_key("HTTP://LOCALHOST:3030/base", "same-id")],
            "元Agent"
        );
        assert_eq!(purposes[&project_scope_key(second, "same-id")], "普通项目");
        assert_ne!(
            session_scope_key(first, "same-id"),
            session_scope_key(second, "same-id")
        );
        assert_eq!(
            session_scope_key(first, "same-id"),
            session_scope_key("HTTP://LOCALHOST:3030/base", "same-id")
        );
    }
}

#[cfg(test)]
mod metrics_tests;
