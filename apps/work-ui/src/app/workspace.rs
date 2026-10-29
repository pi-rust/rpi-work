use dioxus::core::Task;
use dioxus::prelude::*;
use runtime_stats::CompactResult;
use serde_json::Value;
use work_client::{
    AgentClient, AuthState, Capabilities, Diff, FileListing, GitStatus, ModelDescriptor,
    PendingDialog, Project, QueueSnapshot, RuntimeState, SessionSummary, WorkspaceClient,
};
use work_core::{RunPhase, RunProjection};

use super::{
    background::{BtwHost, QueueSync},
    history::{history_has_block, history_id, message_id},
    identity::{node_origin, project_scope_key, session_scope_key},
    runtime::{
        InteractionState, connection_status, conversation_title, default_node_url,
        interaction_state, phase_label, projection_status, resource_snapshot, task_status,
        transport_online,
    },
};
use crate::features::projects::{
    preview::{project_file_kind, render_project_file},
    review::{change_status_label, filter_changed_files, render_diff},
};
use crate::{
    app::{connections, device_mode},
    features::{
        conversation::{
            activity, composer_extras, message_queue, scroll_follow, timeline as conversation,
        },
        meta_agent,
        observability::{runtime_stats, traces},
        projects::{management, project_files, project_picker},
        resources::{library as resources, session_config},
        settings::{self as settings, updates},
    },
    shared::{components, state, theme::appearance},
};
use appearance::{AppearancePicker, use_appearance};
use components::{Icon, Modal};
use management::SessionAction;
use settings::SettingsPage;
use state::{Drafts, update_draft};
const CSS: &str = include_str!("../shared/theme/styles.css");
const THEMES: &str = include_str!("../shared/theme/themes.css");
const RESOURCES_CSS: &str = resources::CSS;
const META_AGENT_CSS: &str = meta_agent::CSS;
const PROJECT_PICKER_CSS: &str = project_picker::CSS;
const TRACES_CSS: &str = traces::CSS;
const CONVERSATION_CSS: &str = conversation::CSS;
const EXTRAS_CSS: &str = composer_extras::CSS;
const QUEUE_CSS: &str = message_queue::CSS;

#[component]
pub(super) fn App() -> Element {
    let appearance = use_appearance();
    let device_mode = device_mode::use_device_mode();
    updates::use_updates();
    let initial_node_url = default_node_url();
    let mut node_connection = connections::use_connections(&initial_node_url);
    let connection_requests = use_signal(Vec::<Task>::new);
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    let mut local_suspension = use_signal(|| None::<tokio::sync::watch::Receiver<bool>>);
    #[allow(unused_mut)] // Mutated only by the browser's same-origin initialization.
    let mut base_url = use_signal(|| initial_node_url.clone());
    let mut shared_client = use_signal(|| {
        super::local_node::client().or_else(|| WorkspaceClient::new(&initial_node_url).ok())
    });
    let mut native_agents = use_signal(std::collections::HashMap::<String, AgentClient>::new);
    let mut node_address_touched = use_signal(|| false);
    let mut search = use_signal(String::new);
    let mut change_query = use_signal(String::new);
    let mut change_status_filter = use_signal(|| "all".to_owned());
    let mut changes_unreviewed = use_signal(|| false);
    let mut reviewed_changes = use_signal(std::collections::HashSet::<String>::new);
    let mut global_search_open = use_signal(|| false);
    let mut picker_search = use_signal(String::new);
    let mut queue_open = use_signal(|| false);
    let mut status = use_signal(|| {
        if cfg!(not(any(target_arch = "wasm32", target_os = "android"))) {
            "正在准备本机工作空间…"
        } else {
            "未连接"
        }
        .to_owned()
    });
    let mut auth_state = use_signal(|| None::<AuthState>);
    let mut password = use_signal(String::new);
    let node_token = use_signal(String::new);
    let mut applied_node_token = use_signal(String::new);
    let mut capabilities = use_signal(|| None::<Capabilities>);
    let mut transport_descriptor = use_signal(|| None::<work_core::AgentDescriptor>);
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
    let mut startup_session = use_signal(|| None::<String>);
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
    let mut inspector_open = use_signal(|| false);
    let mut inspector_preference_ready = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            if let Ok(saved) = document::eval(
                "dioxus.send(localStorage.getItem('rpi-work-inspector-open-v1') === 'true')",
            )
            .recv::<bool>()
            .await
            {
                inspector_open.set(saved);
            }
            inspector_preference_ready.set(true);
        });
    });
    use_effect(move || {
        let ready = *inspector_preference_ready.read();
        #[cfg(target_arch = "wasm32")]
        let open = *inspector_open.read();
        if ready {
            #[cfg(target_arch = "wasm32")]
            {
                let value = if open { "true" } else { "false" };
                let _ = document::eval(&format!(
                    "try{{localStorage.setItem('rpi-work-inspector-open-v1','{value}')}}catch(_){{}}"
                ));
            }
        }
    });
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
            while viewport.recv::<bool>().await.is_ok() {}
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
    let mut new_session_busy = use_signal(|| false);
    let mut new_session_error = use_signal(|| None::<String>);
    let mut auto_titles = use_signal(std::collections::HashMap::<String, String>::new);
    let mut focus_new_session = use_signal(|| None::<String>);
    let mut transcript = use_signal(Vec::<Value>::new);
    let mut projection = use_signal(|| None::<RunProjection>);
    let mut drafts = use_signal(Drafts::new);
    let mut drafts_ready = use_signal(|| false);
    let mut drafts_persistence_enabled = use_signal(|| false);
    let persist_generation = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u64)));
    use_effect(move || {
        spawn(async move {
            match state::load_persisted_drafts().await {
                Ok(mut restored) => {
                    // A keystroke that happened before storage finished loading is
                    // newer than the on-disk snapshot and must win the merge.
                    for (scope, draft) in drafts.read().clone() {
                        if draft.revision > 0 {
                            restored.insert(scope, draft);
                        }
                    }
                    drafts.set(restored);
                    drafts_persistence_enabled.set(true);
                    drafts_ready.set(true);
                }
                Err(error) => {
                    // Do not write an empty map after a read/parse failure: the
                    // damaged file remains available for recovery.
                    status.set(format!("{error} 当前输入仍可编辑，但未能持久化。"));
                    drafts_ready.set(true);
                }
            }
        });
    });
    use_effect(move || {
        let ready = *drafts_ready.read();
        let persistence_enabled = *drafts_persistence_enabled.read();
        let snapshot = drafts.read().clone();
        if !ready || !persistence_enabled {
            return;
        }
        let generation = persist_generation.get().saturating_add(1);
        persist_generation.set(generation);
        let persist_generation = persist_generation.clone();
        spawn(async move {
            state::draft_persist_delay().await;
            if persist_generation.get() != generation {
                return;
            }
            if let Err(error) = state::persist_drafts(&snapshot).await {
                status.set(format!("{error} 当前草稿仍保留在内存中。"));
            }
        });
    });
    let mut sending = use_signal(std::collections::HashSet::<String>::new);
    let mut pending_enter = use_signal(|| None::<(String, u64, u64)>);
    let mut queue = use_signal(|| None::<QueueSnapshot>);
    let queue_drafts = message_queue::use_queue_drafts();
    let attachment_store = composer_extras::use_attachment_store();
    let btw_store = composer_extras::use_btw_store();
    let mut runtime_state = use_signal(|| None::<RuntimeState>);
    let mut available_models = use_signal(Vec::<ModelDescriptor>::new);
    let mut available_thinking_levels = use_signal(Vec::<String>::new);
    let mut last_turn_usage = use_signal(|| None::<(String, Value)>);
    let mut context_open = use_signal(|| false);
    let mut stats_open = use_signal(|| false);
    let mut compact_revision = use_signal(|| 0u64);
    let mut compact_pending = use_signal(|| None::<u64>);
    let mut compact_operation_accepted = use_signal(|| false);
    let mut node_compacting = use_signal(|| false);
    let mut compact_result = use_signal(|| None::<CompactResult>);
    use_effect(move || {
        let generation = *session_generation.read();
        let session_id = selected_session.read().clone();
        let client = shared_client.read().clone();
        let native_agent = selected_session
            .read()
            .as_ref()
            .and_then(|id| native_agents.read().get(id).cloned());
        let descriptor = native_agent
            .as_ref()
            .map(AgentClient::descriptor)
            .or_else(|| transport_descriptor.read().clone());
        available_models.set(Vec::new());
        available_thinking_levels.set(Vec::new());
        let (Some(session_id), Some(client)) = (session_id, client) else {
            return;
        };
        let models_enabled = descriptor
            .as_ref()
            .is_none_or(|value| value.supports("models"));
        let thinking_enabled = descriptor
            .as_ref()
            .is_none_or(|value| value.supports("thinking_levels"));
        state::scoped_spawn(requests, async move {
            let models = if models_enabled {
                if let Some(agent) = native_agent.clone() {
                    agent
                        .models(&session_id)
                        .await
                        .ok()
                        .and_then(|value| serde_json::from_value(value["models"].clone()).ok())
                } else {
                    client.models(&session_id).await.ok()
                }
            } else {
                None
            };
            let levels = if thinking_enabled {
                if let Some(agent) = native_agent {
                    agent
                        .thinking_levels(&session_id)
                        .await
                        .ok()
                        .and_then(|value| serde_json::from_value(value["levels"].clone()).ok())
                } else {
                    client.thinking_levels(&session_id).await.ok()
                }
            } else {
                None
            };
            if *session_generation.read() != generation {
                return;
            }
            if let Some(models) = models {
                available_models.set(models);
            }
            if let Some(levels) = levels {
                available_thinking_levels.set(levels);
            }
        });
    });
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
    let mut active_page = use_signal(|| {
        if cfg!(target_os = "android") {
            "settings"
        } else {
            "workspace"
        }
        .to_owned()
    });
    let mut settings_section = use_signal(|| {
        if cfg!(target_os = "android") {
            "devices"
        } else {
            "settings"
        }
        .to_owned()
    });
    let mut appearance_open = use_signal(|| false);
    let mut session_details_open = use_signal(|| false);
    management::use_menu_behaviour();
    let mut archived_projects = management::use_archived_projects();
    let mut archive_open = use_signal(|| false);
    let mut expanded_projects = management::use_expanded_projects();
    let mut full_project_sessions = use_signal(std::collections::HashSet::<String>::new);
    let sidebar_now = management::use_sidebar_clock();
    let mut session_action_busy = use_signal(std::collections::HashSet::<String>::new);
    let mut rename_target = use_signal(|| None::<(String, String)>);
    let mut rename_error = use_signal(|| None::<String>);
    let mut project_name_busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    let mut undo_archive = use_signal(|| None::<(bool, String)>);
    let mut pending_replies =
        use_signal(std::collections::HashMap::<String, activity::PendingReply>::new);
    // These stores live at App scope: every page and session receives the same binding.
    let mut resource_library = resources::use_resource_library();
    let mut meta_store = meta_agent::use_meta_store();
    let open_session_config = use_signal(|| false);
    let mut project_purposes = use_signal(std::collections::HashMap::<String, String>::new);
    let mut meta_return_page = use_signal(|| "workspace".to_owned());
    let development_associations = use_signal(std::collections::HashMap::<String, String>::new);
    let traces = traces::use_trace_store();
    let mut project_purposes_loaded = use_signal(|| false);

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

    let finish_connection = move |client: WorkspaceClient,
                                  secret: Option<String>,
                                  token: String,
                                  attempt: u64,
                                  url: String| {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        let suspension = local_suspension.peek().clone();
        state::scoped_spawn(connection_requests, async move {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            if let Some(mut suspension) = suspension {
                while !*suspension.borrow_and_update() {
                    if suspension.changed().await.is_err() {
                        return;
                    }
                }
            }
            if *node_connection.epoch.peek() != attempt {
                return;
            }
            shared_client.set(Some(client.clone()));
            let result = connections::open(client, secret).await;
            if *node_connection.epoch.peek() != attempt {
                return;
            }
            match result {
                Ok(connections::ConnectionOutcome::PasswordRequired(auth)) => {
                    applied_node_token.set(token.clone());
                    auth_state.set(Some(auth));
                    node_connection
                        .phase
                        .set(connections::Phase::PasswordRequired);
                    status.set("节点需要密码认证".into());
                    if startup_session.peek().is_some() {
                        active_page.set("settings".into());
                        settings_section.set("devices".into());
                    }
                }
                Ok(connections::ConnectionOutcome::Ready(workspace)) => {
                    let requested = startup_session.write().take();
                    let can_open = requested.as_ref().is_some_and(|id| {
                        workspace
                            .sessions
                            .iter()
                            .any(|session| &session.session_id == id)
                    });
                    auth_state.set(Some(workspace.auth));
                    let transport_label = format!(
                        "{} / {}",
                        workspace.transport.identity.protocol,
                        workspace.transport.identity.transport
                    );
                    transport_descriptor.set(Some(workspace.transport));
                    capabilities.set(Some(workspace.capabilities));
                    sessions.set(workspace.sessions);
                    projects.set(workspace.projects);
                    password.set(String::new());
                    applied_node_token.set(token);
                    if !super::local_node::is_local(&url) {
                        node_connection.history.write().remember(&url);
                    }
                    node_connection.phase.set(connections::Phase::Online);
                    status.set(if super::local_node::is_local(&url) {
                        format!("本机已就绪 · 可打开项目或开始任务 · {transport_label}")
                    } else {
                        format!("在线 · 可选择会话或项目 · {transport_label}")
                    });
                    if let Some(id) = requested {
                        if can_open {
                            pending_open.set(Some((false, id)));
                        } else {
                            status.set("节点已连接，但会话链接不存在或已被删除。".into());
                        }
                    }
                }
                Err(message) => {
                    node_connection
                        .phase
                        .set(connections::Phase::Failed(message.clone()));
                    status.set(message);
                }
            }
        });
    };
    let mut connect_node = move || {
        node_address_touched.set(true);
        if base_url.peek().as_str() == "about:blank" {
            if let Some(error) = super::local_node::startup_error() {
                status.set(error.to_owned());
                node_connection
                    .phase
                    .set(connections::Phase::Failed(error.to_owned()));
                return;
            }
        }
        let local = super::local_node::is_local(&base_url.read());
        let url = if local {
            super::local_node::LOCAL_ADDRESS.to_owned()
        } else {
            match connections::normalize_address(&base_url.read(), device_mode.read().is_mobile()) {
                Ok(address) => format!("{address}/"),
                Err(message) => {
                    status.set(format!("地址无效：{message}"));
                    return;
                }
            }
        };
        let requested_token = node_token.read().trim().to_owned();
        let existing_client =
            if *node_url.peek() == url && *applied_node_token.peek() == requested_token {
                shared_client.peek().clone()
            } else {
                None
            };
        let candidate = match existing_client.map_or_else(
            || {
                if local {
                    super::local_node::client().ok_or_else(|| {
                        work_client::ClientError::Process(
                            super::local_node::startup_error()
                                .unwrap_or("本机工作空间未就绪")
                                .to_owned(),
                        )
                    })
                } else {
                    WorkspaceClient::new_with_token(&url, Some(&requested_token))
                }
            },
            Ok,
        ) {
            Ok(client) => client,
            Err(error) => {
                status.set(format!("地址无效：{error}"));
                return;
            }
        };
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if !local {
            if let Some(prior) = shared_client
                .peek()
                .clone()
                .filter(WorkspaceClient::is_local)
            {
                let (completed, barrier) = tokio::sync::watch::channel(false);
                local_suspension.set(Some(barrier));
                // Not a scoped UI request: cancelling a remote connect must not
                // cancel cleanup. Every subsequent connect awaits this barrier,
                // including a rapid switch back to the static local facade.
                tokio::spawn(async move {
                    prior.suspend().await;
                    let _ = completed.send(true);
                });
            }
        }
        state::cancel_requests(connection_requests);
        new_session_busy.set(false);
        focus_new_session.set(None);
        node_connection.epoch += 1;
        let attempt = *node_connection.epoch.peek();
        base_url.set(url.clone());
        if *node_url.read() != url {
            state::cancel_requests(requests);
            state::cancel_requests(project_requests);
            project_generation += 1;
            project_busy.set(false);
            project_error.set(None);
            state::cancel_requests(submissions);
            session_action_busy.write().clear();
            pending_replies.write().clear();
            rename_target.set(None);
            notice.set(None);
            undo_archive.set(None);
            archive_open.set(false);
            project_name_busy.set(false);
            new_session_busy.set(false);
            focus_new_session.set(None);
            session_generation += 1;
            file_generation += 1;
            change_generation += 1;
            shared_client.set(None);
            let old_native_agents = std::mem::take(&mut *native_agents.write());
            for agent in old_native_agents.into_values() {
                spawn(async move {
                    let _ = agent.stop().await;
                });
            }
            native_agents.set(std::collections::HashMap::new());
            auth_state.set(None);
            transport_descriptor.set(None);
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
        node_connection.address.set(url.clone());
        node_connection.phase.set(connections::Phase::Connecting);
        status.set("连接中…".into());
        finish_connection(candidate, None, requested_token, attempt, url);
    };
    let connect = move |_: MouseEvent| connect_node();

    // Desktop always starts with its own node. Remote history is an explicit
    // choice there; only mobile/Web restore a previously connected computer.
    use_effect(move || {
        if !*node_connection.ready.read() || *node_address_touched.read() {
            return;
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            connect_node();
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            let saved = node_connection.history.peek().recent.first().cloned();
            spawn(async move {
                #[cfg(target_arch = "wasm32")]
            let location = document::eval("const id=new URL(location.href).searchParams.get('session');dioxus.send([location.origin+'/',id]);")
                .recv::<(String, Option<String>)>()
                .await
                .ok();
                #[cfg(target_arch = "wasm32")]
                let fallback = location.as_ref().map(|(origin, _)| origin.clone());
                #[cfg(not(target_arch = "wasm32"))]
                let fallback = None::<String>;
                if *node_address_touched.peek() {
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if let Some((origin, Some(id))) = location {
                    if !id.is_empty()
                        && id.len() <= 128
                        && id
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                    {
                        // A session link is scoped to its serving node, never to a
                        // different node remembered on this device.
                        startup_session.set(Some(id));
                        base_url.set(origin);
                        connect_node();
                        return;
                    }
                }
                if let Some(address) = saved {
                    if connections::normalize_address(&address, device_mode.read().is_mobile())
                        .is_ok()
                    {
                        base_url.set(address);
                        connect_node();
                        return;
                    }
                }
                if let Some(origin) = fallback {
                    base_url.set(origin.clone());
                    node_url.set(origin.clone());
                    node_connection.address.set(origin.clone());
                    shared_client.set(WorkspaceClient::new(&origin).ok());
                }
            });
        }
    });

    let login = move |_: MouseEvent| {
        if node_connection.phase.read().busy() {
            return;
        }
        let draft_url = connections::normalize_address(&base_url.read(), false)
            .map(|value| format!("{value}/"));
        if draft_url.as_deref() != Ok(node_url.read().as_str()) {
            status.set("请先连接当前地址，再登录".into());
            return;
        }
        if node_token.read().trim() != applied_node_token.read().as_str() {
            status.set("连接 Token 已变更，请先重新连接当前地址".into());
            return;
        }
        let Some(client) = shared_client.read().clone() else {
            return;
        };
        let secret = password.read().clone();
        if secret.is_empty() {
            status.set("请输入节点密码".into());
            return;
        }
        state::cancel_requests(connection_requests);
        node_connection.epoch += 1;
        let attempt = *node_connection.epoch.peek();
        node_connection
            .phase
            .set(connections::Phase::Authenticating);
        status.set("正在登录并加载项目…".into());
        // Clear the visible password immediately; the request owns its copy.
        password.set(String::new());
        let token = applied_node_token.peek().clone();
        finish_connection(
            client,
            Some(secret),
            token,
            attempt,
            node_url.read().clone(),
        );
    };
    let mut open_node_settings = move || {
        show_project_form.set(false);
        project_error.set(None);
        active_page.set("settings".to_owned());
        settings_section.set("devices".to_owned());
        sidebar_open.set(false);
        status.set(
            if cfg!(not(any(target_arch = "wasm32", target_os = "android")))
                && (super::local_node::is_local(&node_url.read())
                    || (node_url.read().as_str() == "about:blank"
                        && super::local_node::startup_error().is_some()))
            {
                super::local_node::startup_error()
                    .unwrap_or("本机工作空间暂未就绪，请查看运行状态。")
                    .to_owned()
            } else {
                "请先连接并登录执行节点，再选择项目目录。".to_owned()
            },
        );
    };
    let submit_project = move |intent: project_picker::ProjectIntent| {
        if !matches!(*node_connection.phase.read(), connections::Phase::Online) {
            open_node_settings();
            return;
        }
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
            archived_projects
                .write()
                .remove(&project_scope_key(&node_url.peek(), &project.id));
            show_project_form.set(false);
            pending_open.set(Some((true, project.id)));
            status.set("已复用同目录项目，未覆盖原有用途和名称".into());
            return;
        }
        let generation = *project_generation.read() + 1;
        project_generation.set(generation);
        project_busy.set(true);
        project_error.set(None);
        if let Some(descriptor) = transport_descriptor.read().as_ref()
            && !descriptor.supports("projects")
        {
            status.set(format!(
                "当前 transport {} 不支持项目管理。",
                descriptor.identity.protocol
            ));
            project_busy.set(false);
            return;
        }
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
                if client.is_local() && mode == project_picker::ProjectMode::Create {
                    return client.create_project_directory(&target_name, std::path::Path::new(&target_path), init_git)
                        .await.map_err(|error| error.to_string());
                }
                let (server_name, server_path) = match mode {
                    project_picker::ProjectMode::Open => {
                        (target_name.clone(), Some(target_path.clone()))
                    }
                    project_picker::ProjectMode::Create => {
                        let parent = project_picker::directory_parent(&target_path)
                            .ok_or_else(|| "新建目录必须位于节点项目根目录下。".to_owned())?;
                        let folders = client
                            .project_folders("")
                            .await
                            .map_err(|error| format!("无法读取节点项目根目录：{error}"))?;
                        let name = project_picker::direct_child_name(&folders.root, &target_path)
                            .ok_or_else(|| {
                            format!(
                                "选择的父目录 {parent} 不符合要求；当前节点只支持在项目根目录的直接子目录中新建：{}",
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
    let save_project_alias = move |name: String| {
        if *project_name_busy.peek() {
            return;
        }
        let Some(project_id) = editing_project_id.read().clone() else {
            return;
        };
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        project_name_busy.set(true);
        project_name_error.set(None);
        let scope = project_scope_key(&node_origin(node_url.read().as_str()), &project_id);
        let address = node_url.peek().clone();
        let epoch = *node_connection.epoch.peek();
        state::scoped_spawn(project_requests, async move {
            let result = async {
                super::local_node::authorize(&client, &address).await?;
                client.rename_project(&project_id, &name).await
            }
            .await;
            project_name_busy.set(false);
            if *node_connection.epoch.peek() != epoch {
                return;
            }
            match result {
                Ok(updated) => {
                    if let Some(project) = projects
                        .write()
                        .iter_mut()
                        .find(|project| project.id == project_id)
                    {
                        *project = updated;
                    }
                    project_aliases.write().remove(&scope);
                    if selected_project.peek().as_ref() == Some(&project_id) {
                        selected_project_name.set(Some(name));
                    }
                    project_name_open.set(false);
                    editing_project_id.set(None);
                    notice.set(Some("项目显示名称已更新，磁盘目录保持不变。".into()));
                }
                Err(error) => project_name_error.set(Some(format!(
                    "重命名失败：{}",
                    management::node_error(error)
                ))),
            }
        });
    };
    let mut session_action = move |action: SessionAction| {
        let id = match &action {
            SessionAction::Rename(id)
            | SessionAction::Copy(id)
            | SessionAction::Details(id)
            | SessionAction::Clone(id)
            | SessionAction::Archive(id, _)
            | SessionAction::Pin(id, _)
            | SessionAction::Fork(id, _) => id.clone(),
        };
        let Some(source) = sessions
            .peek()
            .iter()
            .find(|session| session.session_id == id)
            .cloned()
        else {
            return;
        };
        match &action {
            SessionAction::Rename(_) => {
                rename_error.set(None);
                rename_target.set(Some((id, source.title.unwrap_or_default())));
                return;
            }
            SessionAction::Details(_) => {
                if selected_session.peek().as_ref() != Some(&id) {
                    pending_open.set(Some((false, id)));
                }
                session_details_open.set(true);
                return;
            }
            SessionAction::Copy(_) => {
                let literal = serde_json::to_string(&id).unwrap();
                spawn(async move {
                    let result=document::eval(&format!("try{{await navigator.clipboard.writeText({literal});dioxus.send(true);}}catch(_){{dioxus.send(false);}}" )).recv::<bool>().await;
                    notice.set(Some(if matches!(result, Ok(true)) {
                        "已复制会话标识。".into()
                    } else {
                        format!("请手动复制会话标识：{id}")
                    }));
                });
                return;
            }
            _ => {}
        }
        let key = session_scope_key(&node_url.peek(), &id);
        if session_action_busy.peek().contains(&key) {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        let required_capability = match &action {
            SessionAction::Archive(_, _) | SessionAction::Pin(_, _) => Some("session_metadata"),
            SessionAction::Fork(_, _) => Some("fork"),
            SessionAction::Clone(_) => Some("clone"),
            _ => None,
        };
        if let (Some(caps), Some(capability)) = (capabilities.peek().as_ref(), required_capability)
            && let Some(message) = connections::capability_error(caps, capability)
        {
            notice.set(Some(message));
            return;
        }
        session_action_busy.write().insert(key.clone());
        let epoch = *node_connection.epoch.peek();
        let generation = *session_generation.peek();
        let address = node_url.peek().clone();
        let archived_action = matches!(action, SessionAction::Archive(_, true));
        let cloned = matches!(action, SessionAction::Clone(_));
        let fork_entry = match &action {
            SessionAction::Fork(_, entry) => entry.clone(),
            _ => None,
        };
        state::scoped_spawn(submissions, async move {
            let result: Result<_, String> = async {
                super::local_node::authorize(&client, &address)
                    .await
                    .map_err(management::node_error)?;
                match action {
                    SessionAction::Archive(_, archived) => {
                        let preferences = client
                            .session_preferences(&id, Some(archived), None)
                            .await
                            .map_err(management::node_error)?;
                        Ok((
                            None,
                            Some(preferences),
                            if archived {
                                "会话已归档，历史和草稿保留。"
                            } else {
                                "会话已恢复。"
                            }
                            .to_owned(),
                        ))
                    }
                    SessionAction::Pin(_, pinned) => {
                        let preferences = client
                            .session_preferences(&id, None, Some(pinned))
                            .await
                            .map_err(management::node_error)?;
                        Ok((
                            None,
                            Some(preferences),
                            if pinned {
                                "会话已置顶。"
                            } else {
                                "已取消置顶。"
                            }
                            .to_owned(),
                        ))
                    }
                    SessionAction::Fork(_, _) | SessionAction::Clone(_) => {
                        let base = source
                            .title
                            .as_deref()
                            .unwrap_or("会话")
                            .chars()
                            .take(64)
                            .collect::<String>();
                        let title = format!("{} · {}", base, if cloned { "副本" } else { "分支" });
                        let mutation = if cloned {
                            client.clone_session(&id, Some(&title)).await
                        } else {
                            client
                                .fork_session(&id, fork_entry.as_deref(), Some(&title))
                                .await
                        }
                        .map_err(management::node_error)?;
                        let mut branch = client
                            .get_session(&mutation.session_id)
                            .await
                            .unwrap_or_else(|_| source.clone());
                        branch.session_id = mutation.session_id;
                        branch.title = Some(title);
                        branch.archived = false;
                        branch.pinned = false;
                        Ok((
                            Some(branch),
                            None,
                            "已打开新的会话分支，原会话保持不变。".to_owned(),
                        ))
                    }
                    _ => unreachable!(),
                }
            }
            .await;
            session_action_busy.write().remove(&key);
            if *node_connection.epoch.peek() != epoch {
                return;
            }
            match result {
                Ok((branch, preferences, message)) => {
                    if let Some(preferences) = preferences {
                        if let Some(session) = sessions
                            .write()
                            .iter_mut()
                            .find(|session| session.session_id == id)
                        {
                            session.archived = preferences.archived;
                            session.pinned = preferences.pinned;
                        }
                    }
                    if let Some(branch) = branch {
                        let new_id = branch.session_id.clone();
                        {
                            let mut items = sessions.write();
                            items.retain(|session| session.session_id != new_id);
                            items.insert(0, branch);
                        }
                        if *session_generation.peek() == generation {
                            session_details_open.set(false);
                            pending_open.set(Some((false, new_id)));
                        }
                    }
                    undo_archive.set(archived_action.then_some((false, id)));
                    notice.set(Some(message));
                }
                Err(error) => notice.set(Some(format!("操作失败：{error}。原会话保留，可重试。"))),
            }
        });
    };
    let save_session_name = move |title: String| {
        let Some((id, _)) = rename_target.peek().clone() else {
            return;
        };
        let key = session_scope_key(&node_url.peek(), &id);
        if session_action_busy.peek().contains(&key) {
            return;
        }
        let Some(client) = shared_client.peek().clone() else {
            return;
        };
        session_action_busy.write().insert(key.clone());
        rename_error.set(None);
        auto_titles.write().remove(&key);
        let epoch = *node_connection.epoch.peek();
        let address = node_url.peek().clone();
        state::scoped_spawn(submissions, async move {
            let result = async {
                super::local_node::authorize(&client, &address).await?;
                client.rename_session(&id, &title).await
            }
            .await;
            session_action_busy.write().remove(&key);
            if *node_connection.epoch.peek() != epoch {
                return;
            }
            match result {
                Ok(_) => {
                    if let Some(session) = sessions
                        .write()
                        .iter_mut()
                        .find(|session| session.session_id == id)
                    {
                        session.title = Some(title.clone());
                    }
                    if selected_session.peek().as_ref() == Some(&id) {
                        selected_session_title.set(Some(title));
                    }
                    rename_target.set(None);
                    notice.set(Some("会话名称已更新。".into()));
                }
                Err(error) => {
                    rename_error.set(Some(format!("保存失败：{}", management::node_error(error))));
                }
            }
        });
    };
    let mut archive_project = move |id: String, archived: bool| {
        let key = project_scope_key(&node_url.peek(), &id);
        if archived {
            archived_projects.write().insert(key);
        } else {
            archived_projects.write().remove(&key);
        }
        undo_archive.set(archived.then_some((true, id)));
        notice.set(Some(
            if archived {
                "项目已从本设备侧栏归档，文件和会话保留。"
            } else {
                "项目已恢复到侧栏。"
            }
            .into(),
        ));
    };

    let delete_session = move |_: MouseEvent| {
        let Some(session_id) = selected_session.read().clone() else {
            status.set("请先选择会话".into());
            return;
        };
        if let Some(agent) = native_agents.write().remove(&session_id) {
            state::scoped_spawn(requests, async move {
                let _ = agent.stop().await;
                if selected_session.read().as_deref() == Some(session_id.as_str()) {
                    session_generation += 1;
                    selected_session.set(None);
                    selected_session_title.set(None);
                    delete_confirmation.set(false);
                    queue.set(None);
                    runtime_state.set(None);
                    pending_dialogs.set(Vec::new());
                    transcript.set(Vec::new());
                    projection.set(None);
                }
                sessions
                    .write()
                    .retain(|item| item.session_id != session_id);
                status.set("native RPC 会话已停止并移除".into());
            });
            return;
        }
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
        let _ = document::eval("window.__rpiNewFocusDispose?.();");
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
            expanded_projects
                .write()
                .insert(project_scope_key(&node_url.peek(), project_id));
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
        let descriptor = transport_descriptor.read().clone();
        let native_agent = native_agents.read().get(&session_id).cloned();
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => {
                    let connection = if let Some(agent) = native_agent.clone() {
                        agent
                            .session(session_id.clone())
                            .connect()
                            .await
                            .map(work_client::AgentConnection::Native)
                    } else {
                        client
                            .connect_session(session_id.clone())
                            .await
                            .map(work_client::AgentConnection::Http)
                    };
                    match connection {
                        Ok(mut connection) => {
                            if *session_generation.read() != generation {
                                return;
                            }
                            if let Some(project_id) = project_id.as_deref() {
                                if descriptor
                                    .as_ref()
                                    .is_none_or(|value| value.supports("files"))
                                {
                                    if let Ok(listing) = client.files(project_id, "").await {
                                        files.set(Some(listing));
                                    }
                                }
                                if descriptor
                                    .as_ref()
                                    .is_none_or(|value| value.supports("git_diff"))
                                {
                                    if let Ok(repository) = client.git_status(project_id).await {
                                        git_status.set(Some(repository));
                                    }
                                }
                            }
                            if let Some(agent) = native_agent.clone() {
                                if let Ok(value) = agent.queue(&session_id, false).await
                                    && let Ok(snapshot) = serde_json::from_value(value)
                                {
                                    queue.set(Some(snapshot));
                                }
                            } else if descriptor
                                .as_ref()
                                .is_none_or(|value| value.supports("queue"))
                                && let Ok(snapshot) = client.queue(&session_id, false).await
                            {
                                queue.set(Some(snapshot));
                            }
                            if let Some(agent) = native_agent.clone() {
                                if let Ok(value) = agent.state(&session_id).await
                                    && let Ok(state) = serde_json::from_value(value)
                                {
                                    runtime_state.set(Some(state));
                                }
                            } else if let Ok(state) = client.runtime_state(&session_id).await {
                                runtime_state.set(Some(state));
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
                            projection.set(Some(connection.projection().clone()));
                            traces::restore_history(
                                traces,
                                &trace_scope,
                                connection.projection().blocks.clone(),
                            );
                            transcript.set(connection.messages().to_vec());
                            status.set(if let Some(agent) = native_agent.as_ref() {
                                format!(
                                    "会话在线 · {} 已连接",
                                    agent.descriptor().identity.protocol.to_uppercase()
                                )
                            } else {
                                "会话在线 · SSE 已连接".into()
                            });
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
                                        if let Some(pending) =
                                            pending_replies.write().get_mut(&trace_scope)
                                        {
                                            if activity::has_reply(pending, &event.wire) {
                                                pending.responded = true;
                                            }
                                        }
                                        if event.wire.event_type == "connection_reconnecting" {
                                            status.set(
                                                if native_agent.is_some() {
                                                    "native agent 连接中断，正在重试…"
                                                } else {
                                                    "SSE 连接中断，正在重试…"
                                                }
                                                .into(),
                                            );
                                        } else if event.wire.event_type == "connection_online" {
                                            status.set(
                                                if native_agent.is_some() {
                                                    "native agent 已恢复，快照已重新校准"
                                                } else {
                                                    "SSE 已恢复，快照已重新校准"
                                                }
                                                .into(),
                                            );
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
                                            && event
                                                .wire
                                                .data
                                                .get("command")
                                                .and_then(Value::as_str)
                                                == Some("compact")
                                        {
                                            compact_operation_accepted.set(true);
                                            compact_pending.set(Some(*compact_revision.peek()));
                                            compact_result.set(None);
                                            status.set(
                                                "节点已接受压缩请求，等待实际压缩结算…".into(),
                                            );
                                        }
                                        if event_type == "compaction_progress" {
                                            let manual = event
                                                .wire
                                                .data
                                                .get("manual")
                                                .and_then(Value::as_bool)
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
                                            status.set(format!(
                                                "{} · 会话在线",
                                                phase_label(&connection.projection().phase)
                                            ));
                                            let usage = event
                                                .wire
                                                .data
                                                .get("usage")
                                                .filter(|v| v.is_object())
                                                .cloned();
                                            last_turn_usage.set(
                                                usage
                                                    .clone()
                                                    .map(|usage| (trace_scope.clone(), usage)),
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
                                            let refresh_native_agent =
                                                native_agents.read().get(&refresh_session).cloned();
                                            let has_native_agent = refresh_native_agent.is_some();
                                            state::scoped_spawn(requests, async move {
                                                if let Some(agent) = refresh_native_agent.clone() {
                                                    if let Ok(value) =
                                                        agent.queue(&refresh_session, false).await
                                                        && let Ok(snapshot) =
                                                            serde_json::from_value(value)
                                                        && *session_generation.read() == generation
                                                    {
                                                        queue.set(Some(snapshot));
                                                    }
                                                    if let Ok(value) =
                                                        agent.state(&refresh_session).await
                                                        && let Ok(state) =
                                                            serde_json::from_value(value)
                                                        && *session_generation.read() == generation
                                                    {
                                                        runtime_state.set(Some(state));
                                                    }
                                                } else {
                                                    if let Ok(snapshot) = refresh_client
                                                        .queue(&refresh_session, false)
                                                        .await
                                                    {
                                                        if *session_generation.read() == generation
                                                        {
                                                            queue.set(Some(snapshot));
                                                        }
                                                    }
                                                    if let Ok(state) = refresh_client
                                                        .runtime_state(&refresh_session)
                                                        .await
                                                    {
                                                        if *session_generation.read() == generation
                                                        {
                                                            runtime_state.set(Some(state));
                                                        }
                                                    }
                                                }
                                                if !has_native_agent
                                                    && let Ok(items) =
                                                        refresh_client.sessions().await
                                                    && *session_generation.read() == generation
                                                {
                                                    sessions.set(items);
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
                    }
                }
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let mut select_project = move |project_id: String, resume: bool| {
        expanded_projects
            .write()
            .insert(project_scope_key(&node_url.peek(), &project_id));
        let _ = document::eval("window.__rpiNewFocusDispose?.();");
        new_session_error.set(None);
        let recent = sessions
            .read()
            .iter()
            .filter(|s| !s.archived && s.project_id.as_deref() == Some(project_id.as_str()))
            .max_by_key(|s| s.updated_at)
            .map(|s| s.session_id.clone());
        if let Some(id) = recent.filter(|_| resume) {
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
        let descriptor = transport_descriptor.read().clone();
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => {
                    // Directory browsing is deliberately on-demand. Do not recurse through
                    // the project just to populate a client-side catalogue.
                    let files_result = if descriptor
                        .as_ref()
                        .is_none_or(|value| value.supports("files"))
                    {
                        Some(client.files(&project_id, "").await)
                    } else {
                        None
                    };
                    let git_result = if descriptor
                        .as_ref()
                        .is_none_or(|value| value.supports("git_diff"))
                    {
                        Some(client.git_status(&project_id).await)
                    } else {
                        None
                    };
                    if *file_generation.read() != generation {
                        return;
                    }
                    if let Some(files_result) = files_result {
                        match files_result {
                            Ok(listing) => files.set(Some(listing)),
                            Err(error) => status.set(format!("读取项目文件失败：{error}")),
                        }
                    }
                    if let Some(Ok(repository)) = git_result {
                        git_status.set(Some(repository));
                    }
                    status.set("项目文件已加载".into());
                }
                Err(error) => status.set(format!("地址无效：{error}")),
            }
        });
    };

    let mut open_project = move |project_id: String| select_project(project_id, true);
    let mut new_session =
        move |project_id: Option<String>| {
            if *new_session_busy.peek() {
                return;
            }
            if !matches!(*node_connection.phase.peek(), connections::Phase::Online) {
                status.set("运行环境尚未就绪，请稍后重试。".into());
                return;
            }
            let project_id = project_id
                .or_else(|| {
                    selected_project.peek().clone().filter(|id| {
                        !archived_projects
                            .peek()
                            .contains(&project_scope_key(&node_url.peek(), id))
                    })
                })
                .or_else(|| {
                    projects
                        .peek()
                        .iter()
                        .find(|project| {
                            !archived_projects
                                .peek()
                                .contains(&project_scope_key(&node_url.peek(), &project.id))
                        })
                        .map(|project| project.id.clone())
                });
            let Some(project_id) = project_id else {
                project_error.set(None);
                show_project_form.set(true);
                return;
            };
            let Some(client) = shared_client.peek().clone() else {
                return;
            };
            select_project(project_id.clone(), false);
            active_view.set("conversation".into());
            new_session_busy.set(true);
            status.set("正在准备新会话…".into());
            let generation = *session_generation.peek();
            let epoch = *node_connection.epoch.peek();
            let origin = node_url.peek().clone();
            // A project switch must not cancel a successful POST and lose its id.
            state::scoped_spawn(submissions, async move {
                let result =
                    {
                        match client.create_session(Some(&project_id)).await {
                            Ok(id) => Ok(client.get_session(&id).await.unwrap_or_else(|_| {
                                SessionSummary {
                                    session_id: id,
                                    project_id: Some(project_id.clone()),
                                    title: None,
                                    status: "idle".into(),
                                    created_at: 0,
                                    updated_at: 0,
                                    last_seq: 0,
                                    error: None,
                                    pending_resume: false,
                                    latest_settlement: None,
                                    archived: false,
                                    pinned: false,
                                }
                            })),
                            Err(error) => Err(error.to_string()),
                        }
                    };

                if *node_connection.epoch.peek() != epoch {
                    return;
                }
                match result {
                    Ok(mut session) => {
                        let id = session.session_id.clone();
                        if *node_connection.epoch.peek() != epoch {
                            return;
                        }
                        session.title = Some("新会话".into());
                        {
                            let mut items = sessions.write();
                            items.retain(|item| item.session_id != id);
                            items.insert(0, session);
                        }
                        auto_titles
                            .write()
                            .insert(session_scope_key(&origin, &id), String::new());
                        if *session_generation.peek() == generation
                            && selected_project.peek().as_ref() == Some(&project_id)
                        {
                            focus_new_session.set(Some(id.clone()));
                            pending_open.set(Some((false, id)));
                            status.set("新会话已准备好，直接输入你的需求。".into());
                        }
                    }
                    Err(error) => {
                        if *session_generation.peek() == generation {
                            let message = format!("创建会话失败：{error}；可以重试。");
                            new_session_error.set(Some(message.clone()));
                            status.set(message);
                        }
                    }
                }
                new_session_busy.set(false);
            });
        };

    use_effect(move || {
        let requested = focus_new_session.read().clone();
        let selected = selected_session.read().clone();
        let connected = projection.read().is_some();
        if connected && requested.is_some() && requested == selected {
            focus_new_session.set(None);
            let scope = serde_json::to_string(&session_scope_key(
                &node_url.peek(),
                selected.as_deref().unwrap(),
            ))
            .unwrap();
            let _ = document::eval(&format!(
                r"
                window.__rpiNewFocusDispose?.();
                const scope={scope};let timer,observer;
                const dispose=()=>{{clearTimeout(timer);observer?.disconnect();if(window.__rpiNewFocusDispose===dispose)window.__rpiNewFocusDispose=null;}};
                const focus=()=>{{
                    if(document.querySelector('dialog:modal,.workspace-menu:popover-open')){{dispose();return;}}
                    const field=document.querySelector('.composer textarea'),transcript=document.querySelector('.transcript');
                    if(field&&!field.disabled&&transcript?.dataset.sessionScope===scope){{field.focus();dispose();}}
                }};
                window.__rpiNewFocusDispose=dispose;
                observer=new MutationObserver(focus);observer.observe(document.documentElement,{{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','data-session-scope']}});
                timer=setTimeout(dispose,2000);requestAnimationFrame(focus);
            "
            ));
        }
    });

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
        let Some(descriptor) = transport_descriptor.read().clone() else {
            return;
        };
        if !descriptor.supports("files") {
            status.set(format!(
                "当前 transport {} 不支持文件读取。",
                descriptor.identity.protocol
            ));
            return;
        }
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
        let Some(descriptor) = transport_descriptor.read().clone() else {
            return;
        };
        if !descriptor.supports("git_diff") {
            status.set(format!(
                "当前 transport {} 不支持 Git diff。",
                descriptor.identity.protocol
            ));
            return;
        }
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
        if !transport_online(projection.peek().as_ref(), &node_connection.phase.peek()) {
            status.set("节点连接未就绪；草稿已保留，重连并同步后才能发送。".into());
            return;
        }
        if projection.peek().is_none() {
            status.set("正在打开会话，请稍后发送。".into());
            return;
        }
        if sending.read().contains(&draft_scope) {
            return;
        }
        let draft = state::draft_for(&drafts.read(), &draft_scope);
        if draft.text.trim().is_empty() {
            return;
        }
        let Some(client) = shared_client.read().clone() else {
            status.set("请先连接执行节点；草稿已保留。".into());
            return;
        };
        let native_agent = native_agents.read().get(&session_id).cloned();
        if let Some(agent) = native_agent.as_ref()
            && !agent.supports("prompt")
        {
            status.set(format!(
                "当前 native transport {} 不支持 prompt。",
                agent.descriptor().identity.protocol
            ));
            return;
        }
        if let Some(descriptor) = transport_descriptor.read().as_ref()
            && !descriptor.supports("prompt")
        {
            status.set(format!(
                "当前 transport {} 不支持 prompt。",
                descriptor.identity.protocol
            ));
            return;
        }
        if let Some(caps) = capabilities.read().as_ref()
            && let Some(message) = connections::capability_error(caps, "prompt")
        {
            status.set(message);
            return;
        }
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
        pending_replies.write().insert(
            scope.clone(),
            activity::PendingReply {
                text: prompt_text.clone(),
                users_before: activity::users(&transcript.peek()),
                cursor_before: projection.peek().as_ref().and_then(|run| run.cursor),
                accepted: false,
                responded: false,
            },
        );
        scroll_follow::follow_output(&scope);
        sending.write().insert(draft_scope.clone());
        let generation = *session_generation.read();
        state::scoped_spawn(submissions, async move {
            let result = if let Some(agent) = native_agent.clone() {
                agent
                    .prompt(&session_id, &prompt_text)
                    .await
                    .map(|_| true)
                    .map_err(|error| error.to_string())
            } else {
                client
                    .prompt(&session_id, &prompt_text)
                    .await
                    .map(|response| response.success)
                    .map_err(|error| error.to_string())
            };
            sending.write().remove(&draft_scope);
            match result {
                Ok(true) => {
                    if let Some(pending) = pending_replies.write().get_mut(&scope) {
                        pending.accepted = true;
                    }
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
                    let pending_title = auto_titles.peek().get(&scope).cloned();
                    if let Some(saved) = pending_title {
                        let title = if saved.is_empty() {
                            conversation_title(&prompt_text)
                        } else {
                            saved
                        };
                        auto_titles.write().insert(scope.clone(), title.clone());
                        // Naming is best effort after acceptance. Never re-send a
                        // prompt or create another session to recover a title.
                        if native_agent.is_none()
                            && client.rename_session(&session_id, &title).await.is_ok()
                        {
                            auto_titles.write().remove(&scope);
                            if let Some(session) = sessions
                                .write()
                                .iter_mut()
                                .find(|session| session.session_id == session_id)
                            {
                                session.title = Some(title.clone());
                            }
                            if *session_generation.peek() == generation {
                                selected_session_title.set(Some(title));
                            }
                        }
                    }
                }
                _ if *session_generation.read() != generation => {
                    pending_replies.write().remove(&scope);
                }
                Ok(false) => {
                    pending_replies.write().remove(&scope);
                    status.set("指令未被节点接受。".into());
                }
                Err(error) => {
                    pending_replies.write().remove(&scope);
                    status.set(format!("发送失败：{error}"));
                }
            }
        });
    };

    use_effect(move || {
        let id = selected_session.read().clone();
        let Some(id) = id else {
            return;
        };
        let scope = session_scope_key(&node_url.read(), &id);
        let pending = pending_replies.read().get(&scope).cloned();
        let messages = transcript.read().clone();
        if pending.as_ref().is_some_and(|pending| {
            activity::echoed(pending, &messages)
                && (pending.responded || activity::replied_in_history(pending, &messages))
        }) {
            pending_replies.write().remove(&scope);
        }
    });

    let mut deliver_composer = move || {
        let Some(session_id) = selected_session.read().clone() else {
            return;
        };
        let scope = session_scope_key(&node_url.read(), &session_id);
        if !transport_online(projection.peek().as_ref(), &node_connection.phase.peek()) {
            status.set("节点连接未就绪；草稿已保留，重连并同步后才能排队。".into());
            return;
        }
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
        if let Some(caps) = capabilities.read().as_ref()
            && let Some(message) = connections::capability_error(caps, "queue_message")
        {
            status.set(message);
            return;
        }
        let message = draft.text.clone();
        let delivery = queue_delivery.read().clone();
        let native_agent = native_agents.read().get(&session_id).cloned();
        if let Some(agent) = native_agent.as_ref()
            && !agent.supports("queue")
        {
            status.set(format!(
                "当前 native transport {} 不支持 queue_message。",
                agent.descriptor().identity.protocol
            ));
            return;
        }
        let generation = *session_generation.read();
        sending.write().insert(scope.clone());
        status.set("正在投递消息，等待节点确认…".into());
        state::scoped_spawn(submissions, async move {
            let result = if let Some(agent) = native_agent.clone() {
                agent
                    .queue_message(&session_id, &message, &delivery)
                    .await
                    .and_then(|value| {
                        serde_json::from_value(value).map_err(work_client::ClientError::from)
                    })
            } else {
                client.queue_message(&session_id, &message, &delivery).await
            };
            sending.write().remove(&scope);
            match result {
                Ok(response) if response.accepted == Some(true) => {
                    if drafts
                        .read()
                        .get(&scope)
                        .is_some_and(|current| current.revision == draft.revision)
                    {
                        update_draft(drafts, &scope, String::new());
                        let snapshot = if let Some(agent) = native_agent.clone() {
                            agent.queue(&session_id, false).await.and_then(|value| {
                                serde_json::from_value(value)
                                    .map_err(work_client::ClientError::from)
                            })
                        } else {
                            client.queue(&session_id, false).await
                        };
                        if let Ok(snapshot) = snapshot {
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
            if let Some(id) = selected_session.peek().as_deref() {
                update_draft(
                    drafts,
                    &session_scope_key(&node_url.peek(), id),
                    String::new(),
                );
            }
            new_session(None);
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
        let awaiting_reply = selected_session
            .peek()
            .as_ref()
            .and_then(|id| {
                pending_replies
                    .peek()
                    .get(&session_scope_key(&node_url.peek(), id))
                    .cloned()
            })
            .is_some_and(|pending| pending.accepted && !pending.responded);
        let is_running = interaction_state(
            projection.peek().as_ref(),
            transport_online(projection.peek().as_ref(), &node_connection.phase.peek()),
            selected_session.peek().as_ref().is_some_and(|id| {
                sending
                    .peek()
                    .contains(&session_scope_key(&node_url.peek(), id))
            }),
            awaiting_reply,
        )
        .can_queue();
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

    let mut abort = move |_| {
        let Some(session_id) = selected_session.read().clone() else {
            return;
        };
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let native_agent = native_agents.read().get(&session_id).cloned();
        if let Some(agent) = native_agent.as_ref()
            && !agent.supports("abort")
        {
            status.set(format!(
                "当前 native transport {} 不支持 abort。",
                agent.descriptor().identity.protocol
            ));
            return;
        }
        if let Some(descriptor) = transport_descriptor.read().as_ref()
            && !descriptor.supports("abort")
        {
            status.set(format!(
                "当前 transport {} 不支持 abort。",
                descriptor.identity.protocol
            ));
            return;
        }
        if let Some(caps) = capabilities.read().as_ref()
            && let Some(message) = connections::capability_error(caps, "abort")
        {
            status.set(message);
            return;
        }
        state::scoped_spawn(requests, async move {
            let result = if let Some(agent) = native_agent {
                agent.abort(&session_id).await.map(|_| ())
            } else {
                match client_result {
                    Ok(client) => client.abort(&session_id).await.map(|_| ()),
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok(()) => status.set("停止请求已发送，等待 agent_settled…".into()),
                Err(error) => status.set(format!("停止失败：{error}")),
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
        let pending = pending_dialogs
            .read()
            .iter()
            .find(|dialog| dialog.request_id == request_id)
            .cloned();
        let acp_response_id = pending
            .as_ref()
            .and_then(|dialog| dialog.response_id.clone());
        let native_agent = native_agents.read().get(&session_id).cloned();
        if let Some(agent) = native_agent.as_ref()
            && !agent.supports("dialogs")
        {
            status.set(format!(
                "当前 native transport {} 不支持 dialogs。",
                agent.descriptor().identity.protocol
            ));
            return;
        }
        dialog_busy.write().insert(key.clone());
        dialog_errors.write().remove(&key);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        state::scoped_spawn(requests, async move {
            let result = if let Some(agent) = native_agent {
                if agent.descriptor().identity.protocol == "acp" {
                    match acp_response_id {
                        Some(response_id) => match pending
                            .as_ref()
                            .ok_or_else(|| {
                                work_client::ClientError::Acp("missing permission request".into())
                            })
                            .and_then(|dialog| dialog.acp_permission_result(answer.answer.as_ref()))
                        {
                            Ok(result) => agent.respond_to_agent_request(response_id, result).await,
                            Err(error) => Err(error),
                        },
                        None => Err(work_client::ClientError::Unsupported {
                            operation: "session/request_permission".into(),
                            protocol: "acp".into(),
                        }),
                    }
                } else {
                    match answer.answer {
                        Some(value) => agent
                            .dialog_response(&session_id, &request_id, value)
                            .await
                            .map(|_| ()),
                        None => agent
                            .cancel_dialog(&session_id, &request_id)
                            .await
                            .map(|_| ()),
                    }
                }
            } else {
                match client_result {
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
                }
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
    let pending_reply = selected_session.read().as_ref().and_then(|id| {
        pending_replies
            .read()
            .get(&session_scope_key(&node_url.read(), id))
            .cloned()
    });
    let waiting_reply = pending_reply
        .as_ref()
        .is_some_and(|pending| !pending.responded);
    let interaction = interaction_state(
        projection.read().as_ref(),
        transport_online(projection.read().as_ref(), &node_connection.phase.read()),
        submitting,
        waiting_reply,
    );
    let running = interaction.is_active();
    let activity_label = interaction.activity_label(waiting_reply);
    let submitting_original = submitting
        && pending_reply
            .as_ref()
            .is_some_and(|pending| pending.text == composer);
    let composer_loading = interaction.composer_loading(&composer, submitting_original);
    let composer_stop = running && composer.trim().is_empty();
    let query = search.read().trim().to_lowercase();
    let mut visible_sessions = sessions
        .read()
        .iter()
        .filter(|session| {
            !session.archived
                && session.project_id.as_ref().is_none_or(|id| {
                    !archived_projects
                        .read()
                        .contains(&project_scope_key(&node_url.read(), id))
                })
        })
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
    visible_sessions
        .sort_by_key(|session| (!session.pinned, std::cmp::Reverse(session.updated_at)));
    let visible_projects = projects
        .read()
        .iter()
        .filter(|project| {
            !archived_projects
                .read()
                .contains(&project_scope_key(&node_url.read(), &project.id))
        })
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
    let local_workspace = cfg!(not(any(target_arch = "wasm32", target_os = "android")))
        && (super::local_node::is_local(&node_url.read())
            || node_url.read().as_str() == "about:blank");
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
            "providers" => "模型供应商",
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
    let inspector_protocol = capabilities
        .read()
        .as_ref()
        .map_or_else(|| "尚未连接".into(), |caps| caps.protocol.clone());
    let inspector_project_path = selected_project_path
        .read()
        .clone()
        .unwrap_or_else(|| "未选择项目".into());
    let composer_model = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.model_id.clone())
        .unwrap_or_default();
    let composer_thinking = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.thinking_level.clone())
        .unwrap_or_default();
    let model_is_available = available_models
        .read()
        .iter()
        .any(|model| model.id == composer_model);
    let thinking_is_available = available_thinking_levels
        .read()
        .iter()
        .any(|level| level == &composer_thinking);

    let transport_ready =
        transport_online(projection.read().as_ref(), &node_connection.phase.read());
    let (task_tone, task_label) = task_status(
        projection.read().as_ref(),
        &node_connection.phase.read(),
        local_workspace,
    );
    let (connection_tone, node_connection_label) = connection_status(
        projection.read().as_ref(),
        &node_connection.phase.read(),
        local_workspace,
    );

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
            style { {management::CSS} }
            style { {activity::CSS} }
            style { "{runtime_stats::CSS}" }
            runtime_stats::RuntimeDialogs {
                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                runtime_state, projection, transcript, last_turn: last_turn_usage,
                shared_client, native_agents, selected_session, session_generation, compact_revision, compact_pending, node_compacting,
                compact_result, pending_ask: !pending_dialogs.read().is_empty(), stats_open, context_open,
                ontools: move |()| session_details_open.set(true),
            }
            if *queue_open.read() {
                if let Some(session_id) = selected_session.read().clone() {
                    { let scope = session_scope_key(&node_url.read(), &session_id); let expected_scope = scope.clone(); rsx! {
                        message_queue::QueueDialog {
                            scope, session_id: session_id.clone(), client: shared_client,
                            native_agents, drafts: queue_drafts,
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
                    button { class: "new-task", disabled: *new_session_busy.read(), onclick: move |_| new_session(None), Icon { name: "plus" } if *new_session_busy.read() { "正在创建…" } else { "新建任务" } }
                    button { class: "search-trigger", onclick: move |_| { search.set(String::new()); global_search_open.set(true); },
                        Icon { name: "search" } span { "搜索项目与会话" } kbd { "⌘ K" }
                    }
                    nav { class: "primary-nav", aria_label: "主导航",
                        button { class: if active_page.read().as_str() == "workspace" { "nav-item active" } else { "nav-item" }, onclick: move |_| { active_page.set("workspace".to_owned()); sidebar_open.set(false); }, Icon { name: "layout" } "工作空间" }
                        div { class: "nav-group", aria_label: "能力库",
                            div { class: "nav-group-label", "能力库" }
                            for (page, icon, title) in [("extensions", "blocks", "扩展"), ("packages", "blocks", "Packages"), ("skills", "skills", "技能库")] {
                                button { class: if active_page.read().as_str() == page { "nav-item active" } else { "nav-item" }, onclick: move |_| { active_page.set(page.to_owned()); sidebar_open.set(false); }, Icon { name: icon.to_owned() } "{title}" }
                            }
                        }
                    }
                    div { class: "sidebar-projects", role: "navigation", aria_label: "项目与会话",
                    div { class: "section-label", "项目" button { aria_label: "添加项目", title: "添加项目", onclick: move |_| {
                        if matches!(*node_connection.phase.read(), connections::Phase::Online) {
                            project_error.set(None); show_project_form.set(true);
                        } else { open_node_settings(); }
                    }, Icon { name: "plus".to_owned() } } }
                    if let Some(project_id) = selected_project.read().as_ref() {
                        if project_purposes
                            .read()
                            .get(&project_scope_key(&node_origin(node_url.read().as_str()), project_id))
                            .is_some_and(|purpose| purpose == "元Agent") {
                            button { class: if active_page.read().as_str() == "meta-agent" { "nav-item active" } else { "nav-item" }, onclick: move |_| { meta_return_page.set("workspace".into()); active_page.set("meta-agent".into()); sidebar_open.set(false); }, Icon { name: "blocks" } "元Agent" }
                        }
                    }
                    if visible_projects.is_empty() {
                        div { class: "sidebar-empty project-empty",
                            Icon { name: "folder" }
                            strong { if query.is_empty() { "添加你的第一个项目" } else { "没有匹配项目" } }
                            if query.is_empty() { span { "选择工作目录，开始一个会话。" }
                                button { class: "button", onclick: move |_| {
                                    if matches!(*node_connection.phase.read(), connections::Phase::Online) { project_error.set(None); show_project_form.set(true); } else { open_node_settings(); }
                                }, "添加项目" }
                            }
                        }
                    } else {
                        for project in visible_projects.iter() {
                            {
                                let project_id = project.id.clone();
                                let open_project_id = project_id.clone();
                                let create_project_id = project_id.clone();
                                let display_name = project_aliases.read().get(&project_scope_key(&node_origin(node_url.read().as_str()), &project.id)).cloned().unwrap_or_else(|| project.name.clone());
                                let active = selected_project.read().as_ref() == Some(&project_id);
                                let expanded = expanded_projects.read().contains(&project_scope_key(&node_url.read(), &project_id));
                                let show_all = full_project_sessions.read().contains(&project_scope_key(&node_url.read(), &project_id));
                                let project_sessions = visible_sessions
                                    .iter()
                                    .filter(|session| session.project_id.as_deref() == Some(project_id.as_str()))
                                    .cloned()
                                    .collect::<Vec<_>>();
                                rsx! {
                                    div { class: "project-entry", "data-project-id": "{project_id}",
                                    button { class: if expanded { "project-toggle expanded" } else { "project-toggle" },
                                        aria_label: format!("{} {} 的会话", if expanded { "折叠" } else { "展开" }, display_name), aria_expanded: expanded.to_string(),
                                        onclick: { let id=project_id.clone(); move |_| { let key=project_scope_key(&node_url.peek(), &id); if expanded_projects.peek().contains(&key) { expanded_projects.write().remove(&key); } else { expanded_projects.write().insert(key); } } },
                                        Icon { name: "chevron-right" }
                                    }
                                    button {
                                        key: "sidebar-project-{project_id}",
                                        class: if active { "project-row selected" } else { "project-row" },
                                        title: format!("{}\n{}", display_name, project.path),
                                        onclick: move |_| open_project(open_project_id.clone()),
                                        oncontextmenu: { let menu=format!("project-menu-{project_id}"); move |event| { event.prevent_default(); management::open_menu(&menu); } },
                                        Icon { name: "folder" }
                                        span { "{display_name}" }
                                    }
                                    button {
                                        class: "project-new-session", r#type: "button",
                                        aria_label: "在 {display_name} 中新建会话", title: "在此项目中新建会话",
                                        disabled: *new_session_busy.read() || !matches!(*node_connection.phase.read(), connections::Phase::Online),
                                        onclick: move |_| new_session(Some(create_project_id.clone())),
                                        Icon { name: "plus".to_owned() }
                                    }
                                    button {
                                        class: "project-row-menu",
                                        r#type: "button",
                                        aria_label: "{display_name} 的项目操作",
                                        title: "项目操作", "popovertarget": "project-menu-{project_id}",
                                        Icon { name: "more".to_owned() }
                                    }
                                    div { id: "project-menu-{project_id}", class: "workspace-menu", popover: "auto", role: "menu", aria_label: "项目操作",
                                        onkeydown: move |event| event.stop_propagation(),
                                        button { role: "menuitem", disabled: *new_session_busy.read() || !matches!(*node_connection.phase.read(), connections::Phase::Online), onclick: { let id=project_id.clone(); move |_| new_session(Some(id.clone())) }, "新建会话" }
                                        button { role: "menuitem", onclick: { let id=project_id.clone(); move |_| { editing_project_id.set(Some(id.clone())); project_name_error.set(None); project_name_open.set(true); } }, "重命名项目" }
                                        hr {}
                                        button { role: "menuitem", onclick: { let id=project_id.clone(); move |_| archive_project(id.clone(), true) }, "归档项目" }
                                    }
                                    if expanded {
                                        div { class: "sidebar-sessions", aria_label: "会话列表",
                                            for (index, session) in project_sessions.iter().enumerate() {
                                                {
                                                    let id = session.session_id.clone();
                                                    let selected = selected_session.read().as_ref() == Some(&id);
                                                    rsx! {
                                                        if management::session_preview_visible(index, show_all, selected) {
                                                            management::SidebarSession {
                                                                key: "sidebar-session-{id}", session: session.clone(), selected,
                                                                busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &id)),
                                                                now: *sidebar_now.read(), onopen: move |id| open_session(id), onaction: session_action,
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if project_sessions.len() > management::RECENT_SESSION_LIMIT {
                                                {
                                                    let hidden = project_sessions.iter().enumerate().filter(|(index, session)| {
                                                        !management::session_preview_visible(*index, false, selected_session.read().as_ref() == Some(&session.session_id))
                                                    }).count();
                                                    rsx! {
                                                        if show_all || hidden > 0 {
                                                        button { class: "project-session-more", aria_expanded: show_all.to_string(),
                                                            onclick: { let id=project_id.clone(); move |_| {
                                                                let key=project_scope_key(&node_url.peek(), &id);
                                                                if full_project_sessions.peek().contains(&key) { full_project_sessions.write().remove(&key); }
                                                                else { full_project_sessions.write().insert(key); }
                                                            } },
                                                            if show_all { "收起会话" } else { "显示更多（{hidden}）" }
                                                        }
                                                        }
                                                    }
                                                }
                                            }
                                            if project_sessions.is_empty() {
                                                span { class: "sidebar-empty", if query.is_empty() { "还没有会话，点击项目旁的 + 开始" } else { "没有匹配会话" } }
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
                                        management::SidebarSession {
                                            key: "sidebar-session-{id}", session: session.clone(), selected,
                                            busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &id)),
                                            now: *sidebar_now.read(), onopen: move |id| open_session(id), onaction: session_action,
                                        }
                                    }
                                }
                            }
                        }
                    }
                    }
                    button { class: "nav-item archive-entry", onclick: move |_| { archive_open.set(true); sidebar_open.set(false); }, Icon { name: "folder" } "已归档" }
                    button { class: "profile", aria_label: "打开设置", onclick: move |_| { active_page.set("settings".to_owned()); settings_section.set("settings".to_owned()); sidebar_open.set(false); },
                        span { class: "avatar", "R" }
                        span { strong { "设置" } small { if device_mode.read().is_mobile() { "电脑连接与外观" } else { "运行环境、Provider 与外观" } } }
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
                            span { class: "status-badge {task_tone}",
                                i {}
                                "{task_label}"
                            }
                            span { class: "connection-badge {connection_tone}", "{node_connection_label}" }
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
                            if let Some(session)=sessions.read().iter().find(|session| selected_session.read().as_deref() == Some(session.session_id.as_str())) {
                                management::SessionMenu { id: session.session_id.clone(), title: session.title.clone().unwrap_or_default(), archived: session.archived, pinned: session.pinned,
                                    prefix: "header".to_owned(), busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &session.session_id)), onaction: session_action }
                            }
                        }
                    }
                if *show_project_form.read() {
                    project_picker::ProjectPicker {
                        shared_client, projects: projects.read().to_vec(),
                        busy: *project_busy.read(), error: project_error.read().clone(),
                        onsubmit: submit_project,
                        onconnectionrequired: move |()| {
                            if super::local_node::is_local(&node_url.read()) {
                                show_project_form.set(false);
                                base_url.set(node_url.read().clone());
                                connect_node();
                                return;
                            }
                            let password_required = auth_state.peek().as_ref().is_some_and(|auth| auth.required);
                            if let Some(auth) = auth_state.write().as_mut() { auth.authenticated = false; }
                            node_connection.phase.set(if password_required { connections::Phase::PasswordRequired } else { connections::Phase::Failed("节点认证已失效，请重新连接。".to_owned()) });
                            open_node_settings();
                        },
                        onclose: move |()| { project_generation += 1; state::cancel_requests(project_requests); project_busy.set(false); project_error.set(None); show_project_form.set(false); },
                    }
                }
                if *project_name_open.read() {
                    if let Some(project) = project_for_name.clone() {
                        project_picker::ProjectNameDialog {
                            key: "project-name-{project.id}",
                            project: work_client::Project { name: project_aliases.read().get(&project_scope_key(&node_origin(node_url.read().as_str()), &project.id)).cloned().unwrap_or(project.name), ..project },
                            busy: *project_name_busy.read(), error: project_name_error.read().clone(),
                            onsave: save_project_alias,
                            onclose: move |()| { project_name_open.set(false); editing_project_id.set(None); project_name_error.set(None); },
                        }
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
                    SettingsPage { shared_client, requests, active_page, settings_section, base_url, status, auth_state, password, node_token, runtime_state, capabilities, selected_session, session_generation,
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
                        div { class: "section-tabs", role: "tablist", aria_label: "工作区视图",
                            button {
                                id: "conversation-tab",
                                class: if active_view.read().as_str() == "conversation" { "section-tab active" } else { "section-tab" },
                                role: "tab", aria_selected: active_view.read().as_str() == "conversation", aria_controls: "conversation-panel",
                                onclick: move |_| active_view.set("conversation".to_owned()),
                                Icon { name: "message".to_owned() } "对话"
                            }
                            button {
                                id: "changes-tab",
                                class: if active_view.read().as_str() == "changes" { "section-tab active" } else { "section-tab" },
                                role: "tab", aria_selected: active_view.read().as_str() == "changes", aria_controls: "changes-panel",
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
                                        }, shared_client, native_agents, session_generation, requests, runtime_state, status,
                                    }
                                }
                            }
                        }
                        if active_view.read().as_str() == "changes" {
                            div { id: "changes-panel", role: "tabpanel", aria_labelledby: "changes-tab", class: if *change_mobile_preview.read() { "change-workspace show-diff" } else { "change-workspace" },
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
                            div { id: "conversation-panel", role: "tabpanel", aria_labelledby: "conversation-tab", class: "transcript", "data-session-scope": selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                if pending_reply.is_none() && pending_dialogs.read().is_empty() && transcript.read().is_empty() && projection.read().is_none() {
                                    div { class: "empty-state",
                                        if let Some(error) = new_session_error.read().as_ref() { p { role: "alert", "{error}" } }
                                        if selected_session.read().is_some() {
                                            strong { "从这里开始" }
                                            p { "描述你想完成的事情。" }
                                        } else {
                                        strong { if device_mode.read().is_mobile() { "继续你的工作" } else { "开始工作" } }
                                        p { if device_mode.read().is_mobile() { "连接我的电脑，继续项目与会话。" } else { "打开项目，开始新的任务。" } }
                                        if device_mode.read().is_mobile() {
                                            button { class: "button primary", onclick: move |_| { active_page.set("settings".to_owned()); settings_section.set("devices".to_owned()); }, "连接我的电脑" }
                                        } else {
                                            button { class: "button primary", disabled: *new_session_busy.read() || !matches!(*node_connection.phase.read(), connections::Phase::Online), onclick: move |_| {
                                                if projects.read().is_empty() { project_error.set(None); show_project_form.set(true); }
                                                else { new_session(None); }
                                            }, if *new_session_busy.read() { "正在创建…" } else if projects.read().is_empty() { "打开项目" } else { "开始新任务" } }
                                            if *new_session_busy.read() || status.read().starts_with("创建会话失败") || !matches!(*node_connection.phase.read(), connections::Phase::Online) {
                                                p { role: "status", "{status}" }
                                            }
                                        }
                                        }
                                    }
                                } else {
                                    for (index, message) in transcript.read().iter().enumerate() {
                                        div { key: "history-{history_id(message, index)}", class: "message-turn", {
                                            if message.get("role").or_else(|| message.get("kind")).and_then(Value::as_str) == Some("user") {
                                                conversation::render_history(message)
                                            } else {
                                                conversation::render_history_in_context(message, &transcript.read(), projection.read().as_ref().map_or(&[], |run| run.blocks.as_slice()))
                                            }
                                        }
                                        if matches!(message["role"].as_str(), Some("user" | "assistant")) {
                                            if let (Some(entry), Some(id))=(message_id(message), selected_session.read().clone()) {
                                                div { class: "message-actions", button { aria_label: "从这条消息新建分支", onclick: { let entry=entry.to_owned(); move |_| session_action(SessionAction::Fork(id.clone(), Some(entry.clone()))) }, "从此处分支" } }
                                            }
                                        }
                                        }
                                    }
                                    if let Some(pending)=pending_reply.as_ref().filter(|pending| !activity::echoed(pending, &transcript.read())) {
                                        div { class: "pending-user-message", {conversation::render_history(&serde_json::json!({"role":"user","content":pending.text}))} }
                                    }
                                    if let Some(run) = projection.read().as_ref() {
                                        for block in run.blocks.iter().filter(|block| !history_has_block(&transcript.read(), block)) {
                                            div { key: "block-{block.id()}", {conversation::render_message_block(block)} }
                                        }
                                    }
                                    if let Some(label) = activity_label {
                                        activity::Waiting { label, busy: interaction != InteractionState::AwaitingInput }
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
                                disabled: !*drafts_ready.read() || selected_session.read().is_none() || projection.read().is_none(),
                                placeholder: if *new_session_busy.read() && selected_session.read().is_none() { "正在准备新会话…" } else { "继续描述你的想法，或输入 / 使用命令…" },
                                oninput: move |event| if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); update_draft(drafts, &scope, event.value()); },
                                onkeydown: move |event| {
                                    if event.key() == Key::Enter && !event.modifiers().shift() && !event.is_composing() {
                                        event.prevent_default();
                                        // /new changes this signal; release the read guard
                                        // before dispatching a command from the key handler.
                                        let current_session = selected_session.peek().clone();
                                        if let Some(id) = current_session {
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
                                button { class: "button", aria_label: "BTW", onclick: move |_| { if selected_session.read().is_some() { btw_open.set(true); } else { status.set("请先打开主会话，再创建独立旁问。".into()); } }, Icon { name: "message" } "BTW 旁问" }
                                details { class: "capability-menu",
                                summary { Icon { name: "blocks" } "常用能力" }
                                div { class: "capability-menu-items",
                                message_queue::QueueEntry { snapshot: queue.read().clone(), onopen: move |()| queue_open.set(true) }
                                    message_queue::DeliveryPicker { value: queue_delivery.read().clone(), onchange: move |value| queue_delivery.set(value) }
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
                                    if let Some(session_id) = selected_session.read().clone() {
                                        { let scope = session_scope_key(&node_url.read(), &session_id); rsx! { composer_extras::AttachmentPicker { scope, store: attachment_store, presentation: "button".to_owned(), onquote: move |text| {
                                            if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); }
                                        } } } }
                                    }
                                    select {
                                        class: "composer-select",
                                        aria_label: "模型",
                                        value: "{composer_model}",
                                        disabled: available_models.read().is_empty(),
                                        onchange: move |event| {
                                            let value = event.value();
                                            if value.is_empty() { return; }
                                            if let Some(descriptor) = transport_descriptor.read().as_ref()
                                                && !descriptor.supports("models")
                                            {
                                                status.set(format!("当前 transport {} 不支持模型选择。", descriptor.identity.protocol));
                                                return;
                                            }
                                            let (Some(session_id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) else { return; };
                                            let native_agent = native_agents.read().get(&session_id).cloned();
                                            let generation = *session_generation.read();
                                            state::scoped_spawn(requests, async move {
                                                let result = if let Some(agent) = native_agent {
                                                    agent.set_config_option(&session_id, "model", serde_json::json!(value)).await
                                                } else {
                                                    client.set_model(&session_id, &value).await
                                                };
                                                match result {
                                                    Ok(_) => if *session_generation.read() == generation {
                                                        if let Some(agent) = native_agents.read().get(&session_id).cloned() {
                                                            if let Ok(value) = agent.state(&session_id).await
                                                                && let Ok(state) = serde_json::from_value(value)
                                                            { runtime_state.set(Some(state)); }
                                                        } else if let Ok(state) = client.runtime_state(&session_id).await { runtime_state.set(Some(state)); }
                                                        status.set("模型已更新".into());
                                                    },
                                                    Err(error) => status.set(format!("模型更新失败：{error}")),
                                                }
                                            });
                                        },
                                        if !model_is_available {
                                            option { value: "{composer_model}", if composer_model.is_empty() { "模型 · —" } else { "当前模型 · {composer_model}" } }
                                        }
                                        for model in available_models.read().iter() { option { value: "{model.id}", "{model.name} · {model.provider}" } }
                                    }
                                    select {
                                        class: "composer-select composer-thinking-select",
                                        aria_label: "思考等级",
                                        value: "{composer_thinking}",
                                        disabled: available_thinking_levels.read().is_empty(),
                                        onchange: move |event| {
                                            let value = event.value();
                                            if value.is_empty() { return; }
                                            if let Some(descriptor) = transport_descriptor.read().as_ref()
                                                && !descriptor.supports("thinking_levels")
                                            {
                                                status.set(format!("当前 transport {} 不支持思考等级。", descriptor.identity.protocol));
                                                return;
                                            }
                                            let (Some(session_id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) else { return; };
                                            let native_agent = native_agents.read().get(&session_id).cloned();
                                            let generation = *session_generation.read();
                                            state::scoped_spawn(requests, async move {
                                                let result = if let Some(agent) = native_agent {
                                                    agent.set_config_option(&session_id, "thinking", serde_json::json!(value)).await
                                                } else {
                                                    client.set_thinking_level(&session_id, &value).await
                                                };
                                                match result {
                                                    Ok(_) => if *session_generation.read() == generation {
                                                        if let Some(agent) = native_agents.read().get(&session_id).cloned() {
                                                            if let Ok(value) = agent.state(&session_id).await
                                                                && let Ok(state) = serde_json::from_value(value)
                                                            { runtime_state.set(Some(state)); }
                                                        } else if let Ok(state) = client.runtime_state(&session_id).await { runtime_state.set(Some(state)); }
                                                        status.set("思考等级已更新".into());
                                                    },
                                                    Err(error) => status.set(format!("思考等级更新失败：{error}")),
                                                }
                                            });
                                        },
                                        if !thinking_is_available {
                                            option { value: "{composer_thinking}", if composer_thinking.is_empty() { "思考等级 · —" } else { "当前等级 · {composer_thinking}" } }
                                        }
                                        for level in available_thinking_levels.read().iter() { option { value: "{level}", "{level}" } }
                                    }
                                }
                                div { class: "composer-actions",
                                    button {
                                        class: if composer_stop { "send-button is-stop is-loading" } else if composer_loading { "send-button is-loading" } else { "send-button" },
                                        disabled: !transport_ready || selected_session.read().is_none() || projection.read().is_none() || submitting || (!composer_stop && (composer_loading || composer.trim().is_empty())) || interaction == InteractionState::Cancelling,
                                        aria_label: if composer_stop { if interaction == InteractionState::Cancelling { "正在停止…" } else { "停止运行" } } else if composer_loading { activity_label.unwrap_or("正在处理…") } else if interaction.can_queue() { "按所选方式投递消息" } else { "发送消息" },
                                        aria_busy: composer_loading,
                                        title: if composer_stop { "停止当前任务" } else if composer_loading { "等待当前任务完成" } else if interaction.can_queue() { "发送新消息（按所选方式投递）" } else { "发送消息" },
                                        onclick: move |event| { if composer_stop { abort(event); } else { send_or_command(); } },
                                        if composer_loading { activity::Spinner {} }
                                        if composer_stop { span { class: "composer-stop-icon", "aria-hidden": "true" } }
                                        else if !composer_loading { Icon { name: "arrow" } }
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
                            small { class: "inspector-run-status", "{projection_status(projection.read().as_ref(), &node_connection.phase.read(), local_workspace)}" }
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
                                if !device_mode.read().is_mobile() {
                                    button { onclick: move |_| { active_page.set("settings".into()); settings_section.set("providers".into()); }, Icon { name: "server".to_owned() } "Provider" }
                                }
                                button { onclick: move |_| context_open.set(true), Icon { name: "panel".to_owned() } "管理上下文" }
                                button { onclick: move |_| queue_open.set(true), Icon { name: "message".to_owned() } "队列 {queue.read().as_ref().map(|q| q.count).unwrap_or(0)}" }
                            }
                        }
                        div { class: "inspector-card inspector-environment",
                            div { class: "inspector-section-label", strong { "执行环境" } Icon { name: "monitor" } }
                            div { class: "environment-card",
                                div { class: "environment-heading",
                                    span { class: "environment-icon", Icon { name: "monitor" } }
                                    div { strong { if super::local_node::is_local(&node_url.read()) { "这台电脑" } else { "当前执行节点" } } small { "{inspector_protocol}" } }
                                    i { class: if matches!(*node_connection.phase.read(), connections::Phase::Online) { "environment-online" } else { "environment-offline" } }
                                }
                                code { title: "{inspector_project_path}", "{inspector_project_path}" }
                            }
                            div { class: "data-line", span { "模型提供方" } span { "{inspector_provider}" } }
                            if super::local_node::is_local(&node_url.read()) {
                                div { class: "data-line", span { "执行位置" } span { "本机" } }
                            } else {
                                div { class: "data-line", span { "节点认证" } span { if auth_state.read().as_ref().is_some_and(|auth| auth.authenticated) { "已认证" } else { "尚未认证" } } }
                            }
                            button { class: "inspector-small-button environment-settings", title: "{node_url}", onclick: move |_| { active_page.set("settings".into()); settings_section.set("devices".into()); }, if device_mode.read().is_mobile() { "连接我的电脑" } else { "运行环境" } Icon { name: "chevron-right" } }
                        }
                        if !inspector_plan.is_empty() {
                            div { class: "inspector-card",
                                strong { "计划" }
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
                                strong { "工作区变更" }
                                small { "Git 工作区 · 共 {repository.files.len()} 个文件；无本轮归属证据" }
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
                                button { class: "button", onclick: move |_| if let (Some(id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) {
                                    let native_agent = native_agents.read().get(&id).cloned();
                                    state::scoped_spawn(requests, async move {
                                        let result = if let Some(agent) = native_agent {
                                            agent.resume_pending(&id).await
                                        } else {
                                            client.resume_pending(&id).await
                                        };
                                        match result {
                                            Ok(_) => status.set("恢复请求已提交，等待节点结算事件。".into()),
                                            Err(error) => status.set(format!("恢复失败：{error}")),
                                        }
                                    });
                                }, "恢复会话" }
                            }
                        }
                        }
                        div { class: "inspector-footer", span { class: "tiny-logomark", "r↗" } div { span { "你的工具。你的模型。" } span { "你的工作方式。" } } }
                }
                if *appearance_open.read() {
                    AppearancePicker { settings: appearance, onclose: move |()| appearance_open.set(false) }
                }
                if let Some((id, name))=rename_target.read().clone() {
                    management::RenameDialog { key: "rename-{id}", name,
                        busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &id)), error: rename_error.read().clone(),
                        onsave: save_session_name, onclose: move |()| rename_target.set(None) }
                }
                if *archive_open.read() {
                    Modal { id: "archive-browser", title: "已归档", class: "archive-dialog", onclose: move |()| archive_open.set(false),
                        div { class: "modal-body archive-list",
                            p { class: "meta", "项目归档只收纳本设备的侧栏；会话归档保存在节点。文件、历史和草稿均保留。" }
                            h3 { "项目" }
                            for project in projects.read().iter().filter(|project| archived_projects.read().contains(&project_scope_key(&node_url.read(), &project.id))) {
                                div { class: "archive-row", span { "{project.name}" small { "{project.path}" } }
                                    button { class: "button", aria_label: "恢复项目 {project.name}", onclick: { let id=project.id.clone(); move |_| archive_project(id.clone(), false) }, "恢复" }
                                }
                            }
                            h3 { "会话" }
                            for session in sessions.read().iter().filter(|session| session.archived) {
                                div { class: "archive-row", span { "{session.title.as_deref().unwrap_or(\"未命名会话\")}" small { "{session.session_id}" } }
                                    button { class: "button", onclick: { let id=session.session_id.clone(); move |_| { archive_open.set(false); open_session(id.clone()); } }, "打开" }
                                    button { class: "button", aria_label: format!("恢复会话 {}", session.title.as_deref().unwrap_or("未命名会话")), disabled: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &session.session_id)), onclick: { let id=session.session_id.clone(); move |_| session_action(SessionAction::Archive(id.clone(), false)) }, "恢复" }
                                }
                            }
                            if !sessions.read().iter().any(|session| session.archived) { p { class: "meta", "暂无已归档会话。" } }
                        }
                    }
                }
                if let Some(message)=notice.read().clone() {
                    div { class: "workspace-notice", role: "status", span { "{message}" }
                        if let Some((project, id))=undo_archive.read().clone() {
                            button { onclick: move |_| { if project { archive_project(id.clone(), false); } else { session_action(SessionAction::Archive(id.clone(), false)); } undo_archive.set(None); }, "撤销" }
                        }
                        button { aria_label: "关闭操作提示", onclick: move |_| { notice.set(None); undo_archive.set(None); }, "×" }
                    }
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
                            session_config::SessionConfig { shared_client, native_agents, selected_session, session_generation, requests, runtime_state, status, tools_only: false, oncompact: move |()| { session_details_open.set(false); context_open.set(true); } }
                            div { class: "session-actions",
                                button { disabled: selected_session.read().is_none(), onclick: move |_| { let id=selected_session.peek().clone(); if let Some(id)=id { session_action(SessionAction::Rename(id)); } }, "重命名" }
                                button { disabled: selected_session.read().is_none(), onclick: move |_| { let id=selected_session.peek().clone(); if let Some(id)=id { session_action(SessionAction::Fork(id, None)); } }, "新建分支（Fork）" }
                                button { disabled: selected_session.read().is_none(), onclick: move |_| { let id=selected_session.peek().clone(); if let Some(id)=id { session_action(SessionAction::Clone(id)); } }, "复制会话（Clone）" }
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
                        projects: visible_projects.clone(), sessions: visible_sessions.clone(),
                        onproject: move |id| { global_search_open.set(false); open_project(id); },
                        onsession: move |id| { global_search_open.set(false); open_session(id); },
                        onclose: move |()| global_search_open.set(false),
                    }
                }
                if *session_switcher_open.read() {
                    project_picker::SessionSwitcher {
                        projects: visible_projects.clone(), sessions: visible_sessions.clone(),
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
