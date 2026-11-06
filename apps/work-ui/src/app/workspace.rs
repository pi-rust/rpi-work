use dioxus::core::Task;
use dioxus::prelude::*;
use runtime_stats::CompactResult;
use serde_json::Value;
use work_client::{
    AgentBackendOption, AuthState, Capabilities, ClientError, Diff, FileListing, GitStatus,
    HostResponse, ModelDescriptor, PendingDialog, Project, QueueMutation, QueueSnapshot,
    RuntimeState, SessionAccess, SessionAgentBinding, SessionSummary, WorkspaceClient,
};
use work_core::api::realtime::{OperationOutcome, OperationStatus};
use work_core::{RunPhase, RunProjection};

use super::{
    background::{BtwHost, QueueSync},
    config_requests::{self, ConfigField},
    continuation::{self, ContinueAction, ResumeAttempt},
    history::message_id,
    identity::{node_origin, project_scope_key, session_scope_key},
    runtime::{
        InteractionState, SessionObservation, connection_status, conversation_title,
        default_node_url, extension_config_locked, interaction_state, phase_label,
        projection_status, resource_snapshot, sync_sidebar_session, task_status, transport_online,
    },
    transcript_order::{RowSource, TranscriptOrder},
};
use crate::features::projects::{
    preview::{project_file_kind, render_project_file_at},
    review::{CSS as REVIEW_CSS, change_status_label, filter_changed_files, render_diff},
};
use crate::{
    app::{connections, device_mode},
    features::{
        conversation::{
            activity, composer_extras, composer_input, execution_backend, history_pager,
            history_window::HistoryWindow, message_image, message_links, message_queue,
            model_picker, output_motion, scroll_follow, timeline as conversation, turn_timing,
            user_timeline,
        },
        meta_agent,
        observability::{runtime_stats, traces},
        projects::{management, node_info, project_files, project_picker},
        resources::{library as resources, session_config},
        settings::{self as settings, updates},
    },
    shared::{brand::BrandMark, components, state, theme::appearance},
};
use appearance::{AppearancePicker, use_appearance};
use components::{Icon, Modal};
use management::SessionAction;
use settings::SettingsPage;
use state::{Drafts, update_draft};
const CSS: &str = include_str!("../shared/theme/styles.css");
const THEMES: &str = appearance::STYLESHEETS;
const RESOURCES_CSS: &str = resources::CSS;
const META_AGENT_CSS: &str = meta_agent::CSS;
const PROJECT_PICKER_CSS: &str = project_picker::CSS;
const TRACES_CSS: &str = traces::CSS;
const CONVERSATION_CSS: &str = conversation::CSS;
const EXTRAS_CSS: &str = composer_extras::CSS;
const QUEUE_CSS: &str = message_queue::CSS;

async fn reconcile_operation_value(
    client: &WorkspaceClient,
    operation: work_core::api::realtime::OperationRef,
) -> Result<Value, String> {
    let receipt = format!("{}/{}", operation.owner_epoch, operation.operation_id);
    match client.operation_status(&operation).await {
        Ok(OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        }) => Ok(value),
        Ok(OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed { .. },
        }) => Err(format!(
            "节点已记录失败（receipt {receipt}）；本地附件仍保留，请修正后再发送。"
        )),
        Ok(
            OperationStatus::OutcomeUnknown
            | OperationStatus::Reserved
            | OperationStatus::Dispatching,
        ) => Err(format!(
            "执行结果尚未确认（receipt {receipt}）；未重发，本地附件仍保留，请重连后查询。"
        )),
        Err(error) => Err(format!(
            "执行结果未确认（receipt {receipt}），查询失败：{error}；未重发，本地附件仍保留。"
        )),
    }
}

async fn reconcile_prompt_response(
    client: &WorkspaceClient,
    response: Result<HostResponse<Value>, ClientError>,
) -> Result<HostResponse<Value>, String> {
    match response {
        Ok(response) => Ok(response),
        Err(ClientError::OperationOutcomeUnknown { operation }) => {
            let receipt = format!("{}/{}", operation.owner_epoch, operation.operation_id);
            reconcile_operation_value(client, operation).await
                .and_then(|value| serde_json::from_value(value).map_err(|_| format!("执行结果已记录但响应格式异常（receipt {receipt}）；附件未清理，请核对历史。")))
        }
        Err(error) => Err(error.to_string()),
    }
}

#[component]
pub(super) fn App() -> Element {
    // Opt-in E2E probe counts actual App executions, not DOM mutations. The
    // one-time handshake leaves only a Cell check in normal WASM renders;
    // document::eval is never called per render unless the test opts in at boot.
    #[cfg(target_arch = "wasm32")]
    {
        let enabled = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(false)));
        let probe_enabled = enabled.clone();
        use_effect(move || {
            let enabled = probe_enabled.clone();
            spawn(async move {
                if document::eval("dioxus.send(window.__rpiAppRenderProbe?.enabled === true);")
                    .recv::<bool>()
                    .await
                    .unwrap_or(false)
                {
                    enabled.set(true);
                    let _ = document::eval("window.__rpiAppRenderProbe.ready = true;");
                }
            });
        });
        if enabled.get() {
            let _ = document::eval("window.__rpiAppRenderProbe.appRenders++;");
        }
    }
    let appearance = use_appearance();
    crate::features::observability::footer_plugins::use_footer_preferences();
    let device_mode = device_mode::use_device_mode();
    updates::use_updates();
    let entry_node = use_hook(try_consume_context::<super::entry::EntryNode>);
    let navigation = use_hook(try_consume_context::<super::node_navigation::NodeNavigation>);
    let initial_node_url = entry_node.as_ref().map_or_else(default_node_url, |node| {
        format!("{}/", node.address.trim_end_matches('/'))
    });
    let entry_address = use_signal(|| entry_node.as_ref().map(|node| node.address.clone()));
    let mut node_connection = connections::use_connections(&initial_node_url);
    let connection_requests = use_signal(Vec::<Task>::new);
    let mut catalog_refresh = use_signal(super::workspace_sync::RefreshState::default);
    use_effect(move || {
        let epoch = *node_connection.epoch.read();
        let online = matches!(*node_connection.phase.read(), connections::Phase::Online);
        if catalog_refresh.peek().epoch != epoch || !online {
            catalog_refresh.set(super::workspace_sync::RefreshState {
                epoch,
                busy: online,
                message: if online {
                    "正在读取会话与项目…".into()
                } else {
                    String::new()
                },
                ..Default::default()
            });
        }
    });
    let mut base_url = use_signal(|| initial_node_url.clone());
    let mut shared_client = use_signal(|| {
        if let Some(node) = &entry_node {
            // Preserve the exact authenticated transport (Android's cookie jar
            // is client-owned); rebuilding it would discard QR authorization.
            Some(node.client.clone())
        } else if super::local_node::is_local(&initial_node_url) {
            super::local_node::client()
        } else {
            WorkspaceClient::new(&initial_node_url).ok()
        }
    });
    // Observation only; execution and protocol adaptation remain node-owned.
    let mut session_access = use_signal(|| None::<SessionAccess>);
    let mut session_access_key = use_signal(|| None::<(u64, u64, String)>);
    let mut session_observations =
        use_signal(std::collections::HashMap::<(u64, String), SessionObservation>::new);
    use_effect(move || {
        let epoch = *node_connection.epoch.read();
        session_observations
            .write()
            .retain(|(observed_epoch, _), _| *observed_epoch == epoch);
    });
    let mut backend_options = use_signal(Vec::<AgentBackendOption>::new);
    let mut selected_backend = use_signal(|| "rpi".to_owned());
    let mut session_binding = use_signal(|| None::<SessionAgentBinding>);
    let mut backend_catalog_generation = use_signal(|| 0u64);
    use_effect(move || {
        let client = shared_client.read().clone();
        let online = matches!(*node_connection.phase.read(), connections::Phase::Online);
        let epoch = *node_connection.epoch.read();
        let generation = (*backend_catalog_generation.peek()).saturating_add(1);
        backend_catalog_generation.set(generation);
        backend_options.set(Vec::new());
        selected_backend.set("rpi".into());
        session_binding.set(None);
        if let Some(client) = client.filter(|_| online) {
            spawn(async move {
                let result = client.agent_backends().await;
                if *backend_catalog_generation.peek() != generation
                    || *node_connection.epoch.peek() != epoch
                {
                    return;
                }
                match result {
                    Ok(options) => backend_options.set(options),
                    Err(error) => backend_options.set(vec![AgentBackendOption {
                        id: "rpi".into(),
                        name: "RPI".into(),
                        available: false,
                        reason: Some(format!("无法读取后端目录：{error}")),
                    }]),
                }
            });
        }
    });
    let mut node_address_touched = use_signal(|| false);
    let mut change_query = use_signal(String::new);
    let mut change_status_filter = use_signal(|| "all".to_owned());
    let mut changes_unreviewed = use_signal(|| false);
    let mut reviewed_changes = use_signal(std::collections::HashSet::<String>::new);
    let mut global_search_open = use_signal(|| false);
    let mut status = use_signal(|| {
        if cfg!(not(any(target_arch = "wasm32", target_os = "android"))) {
            "正在准备本机工作空间…"
        } else {
            "未连接"
        }
        .to_owned()
    });
    let mut auth_state = use_signal(|| None::<AuthState>);
    let mut password = use_signal(connections::NodePassword::default);
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
    let mut file_image = use_signal(|| None::<Result<message_image::ImageBlock, String>>);
    let mut git_status = use_signal(|| None::<GitStatus>);
    let mut git_loading = use_signal(|| false);
    let mut git_error = use_signal(|| None::<String>);
    let mut git_generation = use_signal(|| 0u64);
    let mut git_refresh = use_signal(|| 0u64);
    let mut git_scope = use_signal(|| None::<(String, Option<String>)>);
    let mut change_staged = use_signal(|| false);
    let mut selected_project_file = use_signal(|| None::<String>);
    // Metadata requests must not be invalidated by preview reads or scope cleanup.
    let mut file_generation = use_signal(|| 0u64);
    let mut preview_generation = use_signal(|| 0u64);
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
    let mut blocked_history = use_signal(|| None::<Value>);
    let mut startup_session = use_signal(|| None::<String>);
    let mut default_session_open = use_signal(|| None::<(u64, String)>);
    let mut node_url = use_signal(|| initial_node_url.clone());
    let (node_probe, refresh_node_probe) = node_info::use_node_probe(
        shared_client,
        node_url,
        node_connection.epoch,
        node_connection.phase,
    );
    let mut node_info_target = use_signal(|| None::<(String, u64, Option<String>)>);
    use_effect(move || {
        let address = node_url.read().clone();
        let epoch = *node_connection.epoch.read();
        if node_info_target
            .peek()
            .as_ref()
            .is_some_and(|(old_address, old_epoch, _)| {
                *old_address != address || *old_epoch != epoch
            })
        {
            node_info_target.set(None);
        }
    });
    let mut show_node_info = move |project_id: Option<String>| {
        let _ = document::eval(
            r#"
            const menu = document.activeElement?.closest?.('.workspace-menu');
            if (menu) {
                const trigger = document.querySelector('[popovertarget="'+CSS.escape(menu.id)+'"]');
                menu.hidePopover(); trigger?.focus({preventScroll:true});
            }
        "#,
        );
        node_info_target.set(Some((
            node_url.peek().clone(),
            *node_connection.epoch.peek(),
            project_id,
        )));
    };
    let mut active_view = use_signal(|| "conversation".to_owned());
    let mut sidebar_open = use_signal(|| false);
    use_effect(move || {
        let open = *sidebar_open.read();
        spawn(async move {
            let script = format!(
                r"
                const side = document.querySelector('.sidebar');
                const menu = document.querySelector('.mobile-menu');
                const sync = () => {{ side.inert = matchMedia('(max-width:767px)').matches && !{open}; }};
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
                const phone = matchMedia('(max-width:767px)');
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
    let mut project_file_lines = use_signal(|| None::<(usize, usize)>);
    let mut reported_files = use_signal(|| (String::new(), Vec::<work_client::FileEntry>::new()));
    let mut project_file_mode = use_signal(|| "preview".to_owned());
    let mut project_file_wrap = use_signal(|| true);
    let mut file_loading = use_signal(|| false);
    let mut file_error = use_signal(|| None::<String>);
    let mut change_directory_filter = use_signal(String::new);
    let mut change_mobile_preview = use_signal(|| false);
    let mut session_switcher_open = use_signal(|| false);
    let mut new_session_busy = use_signal(|| false);
    let mut new_session_follow = use_signal(|| false);
    let mut new_session_error = use_signal(|| None::<String>);
    let mut auto_titles = use_signal(std::collections::HashMap::<String, String>::new);
    let mut focus_new_session = use_signal(|| None::<String>);
    let mut transcript = use_signal(Vec::<Value>::new);
    let transcript_order =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(TranscriptOrder::default())));
    let mut history_window = use_signal(HistoryWindow::default);
    let mut projection = use_signal(|| None::<RunProjection>);
    let mut drafts = use_signal(Drafts::new);
    let mut drafts_ready = use_signal(|| false);
    let mut drafts_persistence_enabled = use_signal(|| false);
    let mut draft_persist_task = use_signal(|| None::<Task>);
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
                    state::initialize_draft_flush(&restored);
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
        // Subscribe without cloning every saved conversation on every keystroke.
        let _ = drafts.read();
        if let Some(task) = draft_persist_task.write().take() {
            task.cancel();
        }
        if !ready || !persistence_enabled {
            return;
        }
        draft_persist_task.set(Some(spawn(async move {
            state::draft_persist_delay().await;
            let snapshot = drafts.peek().clone();
            if let Err(error) = state::persist_drafts(&snapshot).await {
                status.set(format!("{error} 当前草稿仍保留在内存中。"));
            }
        })));
    });
    let mut sending = use_signal(std::collections::HashSet::<String>::new);
    // Each request has a distinct ticket; acceptance still waits for real settlement.
    let mut resuming =
        use_signal(std::collections::HashMap::<(String, u64, u64), ResumeAttempt>::new);
    let mut resume_revision = use_signal(|| 0u64);
    let mut pending_enter = use_signal(|| None::<(String, u64, u64)>);
    let mut queue = use_signal(|| None::<QueueSnapshot>);
    let mut queue_revision = use_signal(|| 0u64);
    let mut queue_busy = use_signal(std::collections::HashSet::<String>::new);
    let mut queue_errors = use_signal(std::collections::HashMap::<String, String>::new);
    let mut queue_history = use_signal(std::collections::HashMap::<String, QueueSnapshot>::new);
    // Withdrawals outlive navigation so text returns to its original main draft.
    let queue_requests = use_signal(Vec::<Task>::new);
    let mut attachment_store = composer_extras::use_attachment_store();
    let btw_store = composer_extras::use_btw_store();
    let mut runtime_state = use_signal(|| None::<RuntimeState>);
    let mut available_models = use_signal(Vec::<ModelDescriptor>::new);
    let mut model_catalog_loading = use_signal(|| false);
    let mut model_catalog_error = use_signal(|| None::<String>);
    let mut model_catalog_readable = use_signal(|| false);
    let mut model_catalog_writable = use_signal(|| false);
    let mut model_catalog_revision = use_signal(|| 0u64);
    let mut model_catalog_ticket = use_signal(|| 0u64);
    let mut composer_configuration = use_signal(|| None::<work_client::SessionConfiguration>);
    // The bound protocol is stable within a context. Keep its ACP wrapper
    // mounted through a refresh so an in-flight setter retains its busy guard.
    let mut composer_protocol = use_signal(|| None::<((u64, u64, Option<String>), String)>);
    let mut model_commit_revision = use_signal(|| 0u64);
    let mut model_pending = use_signal(|| None::<(u64, u64, Option<String>)>);
    let mut thinking_pending = use_signal(|| None::<(u64, u64, Option<String>)>);
    let mut thinking_writable = use_signal(|| false);
    let mut thinking_commit_revision = use_signal(|| 0u64);
    let mut thinking_error = use_signal(|| None::<((u64, u64, Option<String>), String)>);
    let mut thinking_loading = use_signal(|| false);
    let mut thinking_load_error = use_signal(|| None::<String>);
    let mut config_unconfirmed = use_signal(|| None::<((u64, u64, Option<String>), ConfigField)>);
    let model_context = move || {
        (
            *session_generation.read(),
            *node_connection.epoch.read(),
            selected_session.read().clone(),
        )
    };
    let resume_key = move || {
        selected_session.read().as_ref().map(|id| {
            (
                session_scope_key(&node_url.read(), id),
                *session_generation.read(),
                *node_connection.epoch.read(),
            )
        })
    };
    use_effect(move || {
        let current = resume_key();
        resuming
            .write()
            .retain(|key, _| Some(key) == current.as_ref());
    });
    let config_needs_verification = move || {
        config_unconfirmed
            .read()
            .as_ref()
            .is_some_and(|(scope, _)| scope == &model_context())
    };
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
        let context = model_context();
        let _ = *model_catalog_revision.read();
        let ticket = *model_catalog_ticket.peek() + 1;
        model_catalog_ticket.set(ticket);
        let session_id = selected_session.read().clone();
        let client = shared_client.read().clone();
        available_models.set(Vec::new());
        available_thinking_levels.set(Vec::new());
        model_catalog_error.set(None);
        composer_configuration.set(None);
        if composer_protocol
            .peek()
            .as_ref()
            .is_some_and(|(scope, _)| scope != &context)
        {
            composer_protocol.set(None);
        }
        model_catalog_readable.set(false);
        model_catalog_writable.set(false);
        thinking_writable.set(false);
        model_catalog_loading.set(false);
        thinking_loading.set(false);
        thinking_load_error.set(None);
        let (Some(session_id), Some(client)) = (session_id, client) else {
            return;
        };
        model_catalog_loading.set(true);
        thinking_loading.set(true);
        state::scoped_spawn(requests, async move {
            let result = config_requests::bounded(
                "读取会话配置能力",
                client.session_configuration(&session_id),
            )
            .await;
            if model_context() != context || *model_catalog_ticket.peek() != ticket {
                return;
            }
            let configuration = match result {
                Ok(configuration) => configuration,
                Err(error) => {
                    model_catalog_loading.set(false);
                    thinking_loading.set(false);
                    let message = format!("读取会话配置失败：{error}；可刷新重试");
                    model_catalog_error.set(Some(message.clone()));
                    thinking_load_error.set(Some(message));
                    return;
                }
            };
            let access = config_requests::CatalogAccess::for_configuration(&configuration);
            composer_protocol.set(Some((context.clone(), configuration.protocol.clone())));
            composer_configuration.set(Some(configuration));
            model_catalog_readable.set(access.models_read);
            model_catalog_writable.set(access.models_write);
            thinking_writable.set(access.thinking_write);
            model_catalog_loading.set(access.models_read);
            thinking_loading.set(access.thinking_read);
            if access.models_read {
                let id = session_id.clone();
                let client = client.clone();
                let context = context.clone();
                state::scoped_spawn(requests, async move {
                    let result = config_requests::bounded("读取模型列表", client.models(&id)).await;
                    if model_context() != context || *model_catalog_ticket.peek() != ticket {
                        return;
                    }
                    model_catalog_loading.set(false);
                    match result {
                        Ok(models) => available_models.set(models),
                        Err(error) => {
                            model_catalog_error.set(Some(format!("读取模型列表失败：{error}")));
                        }
                    }
                });
            }
            // Catalog readiness must not wait for an unrelated thinking-level request.
            if access.thinking_read {
                state::scoped_spawn(requests, async move {
                    let result = config_requests::bounded(
                        "读取思考等级",
                        client.thinking_levels(&session_id),
                    )
                    .await;
                    if model_context() != context || *model_catalog_ticket.peek() != ticket {
                        return;
                    }
                    thinking_loading.set(false);
                    match result {
                        Ok(levels) => available_thinking_levels.set(levels),
                        Err(error) => {
                            thinking_load_error.set(Some(format!("读取思考等级失败：{error}")));
                        }
                    }
                });
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
    settings::use_pairing();
    let mut active_page = use_signal(|| "workspace".to_owned());
    // Leaving the composer abandons automatic navigation for this attempt.
    // Returning before its response arrives must not restore that intent.
    use_effect(move || {
        if *new_session_follow.read()
            && (active_page.read().as_str() != "workspace"
                || active_view.read().as_str() != "conversation")
        {
            new_session_follow.set(false);
        }
    });
    let mut settings_section = use_signal(|| "settings".to_owned());
    let mut appearance_open = use_signal(|| false);
    let mut session_details_open = use_signal(|| false);
    let mut session_settings_tab = use_signal(|| "details".to_owned());
    let mut timeline_visible = use_signal(|| false);
    management::use_menu_behaviour();
    let mut archived_projects = management::use_archived_projects();
    let mut archive_open = use_signal(|| false);
    let mut expanded_projects = management::use_expanded_projects();
    let mut full_project_sessions = use_signal(std::collections::HashSet::<String>::new);
    let sidebar_now = management::use_sidebar_clock();
    let mut session_action_busy = use_signal(std::collections::HashSet::<String>::new);
    let mut opening_session = use_signal(|| None::<(String, u64)>);
    let mut rename_target = use_signal(|| None::<(String, String)>);
    let mut rename_error = use_signal(|| None::<String>);
    let mut project_name_busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    let mut undo_archive = use_signal(|| None::<(bool, String)>);
    let mut pending_replies =
        use_signal(std::collections::HashMap::<String, activity::PendingReply>::new);
    let mut stopping = use_signal(std::collections::HashSet::<String>::new);
    // These stores live at App scope: every page and session receives the same binding.
    let mut resource_library = resources::use_resource_library();
    let mut meta_store = meta_agent::use_meta_store();
    let open_session_config = use_signal(|| false);
    // SkillPicker requests enter the design tab of the same settings dialog.
    // The embedded resource view consumes the request after it mounts.
    use_effect(move || {
        if *open_session_config.read() {
            session_settings_tab.set("design".into());
            session_details_open.set(true);
        }
    });
    let mut project_purposes = use_signal(std::collections::HashMap::<String, String>::new);
    let mut meta_return_page = use_signal(|| "workspace".to_owned());
    let development_associations = use_signal(std::collections::HashMap::<String, String>::new);
    let traces = traces::use_trace_store();
    let timing_scope = use_memo(move || {
        selected_session
            .read()
            .as_ref()
            .map(|id| session_scope_key(&node_url.read(), id))
            .unwrap_or_default()
    });
    use_context_provider(|| turn_timing::TimingContext {
        store: traces,
        scope: timing_scope,
    });
    let output_motion = output_motion::use_output_motion(timing_scope);
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

    let finish_connection =
        move |client: WorkspaceClient, secret: Option<String>, attempt: u64, url: String| {
            state::scoped_spawn(connection_requests, async move {
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
                        // A retained same-address session belongs to the old epoch.
                        // Reopen it through the normal access/binding observation path,
                        // just like an explicit startup session, before exposing commands.
                        let requested = startup_session
                            .write()
                            .take()
                            .or_else(|| selected_session.peek().clone());
                        let automatic = requested.is_none();
                        let requested = requested.or_else(|| {
                            connections::default_session_id(
                                &workspace.sessions,
                                &workspace.projects,
                                |id| {
                                    archived_projects
                                        .peek()
                                        .contains(&project_scope_key(&url, id))
                                },
                            )
                        });
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
                        password.set(connections::NodePassword::default());
                        node_connection.history.write().remember(&url);
                        node_connection.phase.set(connections::Phase::Online);
                        status.set(if super::local_node::is_local(&url) {
                            format!("本机已就绪 · 可打开项目或开始任务 · {transport_label}")
                        } else {
                            format!("在线 · 可选择会话或项目 · {transport_label}")
                        });
                        if let Some(id) = requested {
                            if can_open {
                                if automatic {
                                    default_session_open.set(Some((attempt, id.clone())));
                                }
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
        if let Some(navigation) = navigation {
            let target = match connections::normalize_address(&base_url.peek(), false) {
                Ok(address) => address,
                Err(error) => {
                    status.set(error);
                    return;
                }
            };
            if entry_address
                .peek()
                .as_deref()
                .map(|entry| entry.trim_end_matches('/'))
                != Some(target.as_str())
            {
                // A hosted browser changes nodes at their own Web origin. Never
                // build a cross-origin API client or forward a password from here.
                password.set(connections::NodePassword::default());
                navigation.navigate.call(target);
                return;
            }
        }
        #[cfg(target_os = "android")]
        if connections::normalize_address(&base_url.peek(), false).ok()
            != entry_address
                .peek()
                .as_ref()
                .and_then(|address| connections::normalize_address(address, false).ok())
        {
            status.set("Android App 需要扫码授权，请在“我的电脑”中扫码更换电脑。".into());
            return;
        }
        let local = super::local_node::is_local(&base_url.read());
        let url = if local {
            super::local_node::LOCAL_ADDRESS.to_owned()
        } else {
            match connections::normalize_connection_address(
                &base_url.read(),
                device_mode.read().is_mobile(),
                entry_address.read().as_deref(),
            ) {
                Ok(address) => format!("{address}/"),
                Err(message) => {
                    status.set(format!("地址无效：{message}"));
                    return;
                }
            }
        };
        let existing_client = if *node_url.peek() == url {
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
                    WorkspaceClient::new(&url)
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
        // Consume only the password entered for this exact node; navigation
        // must never forward a previous node's draft to another address.
        let secret = password.write().take_for(&url);
        // Navigation detaches this UI, not the node-owned execution processes.
        // LocalGuard remains responsible for shutting down the embedded owner
        // only when the desktop process exits.
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
            stopping.write().clear();
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
            session_access.set(None);
            session_access_key.set(None);
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
            session_binding.set(None);
            selected_session_title.set(None);
            files.set(None);
            file_content.set(None);
            file_image.set(None);
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
            sending.set(std::collections::HashSet::new());
            node_url.set(url.clone());
        }
        node_connection.address.set(url.clone());
        node_connection.phase.set(if secret.is_some() {
            connections::Phase::Authenticating
        } else {
            connections::Phase::Connecting
        });
        status.set("连接中…".into());
        finish_connection(candidate, secret, attempt, url);
    };
    let connect = move |()| connect_node();

    // A successful node choice survives restarts. Explicit desktop configuration
    // wins over history; failure never silently selects another data owner.
    use_effect(move || {
        if !*node_connection.ready.read() || *node_address_touched.read() {
            return;
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let explicit = super::local_node::requested_address();
            let address = connections::startup_address(
                explicit,
                &node_connection.history.peek(),
                super::local_node::address().unwrap_or("about:blank"),
                true,
            );
            base_url.set(address);
            connect_node();
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            let history = node_connection.history.peek().clone();
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
                if let Some(address) = entry_address.peek().clone() {
                    // Entry already authenticated this owner. History must never
                    // redirect hosted Web/Android to another remembered node.
                    #[cfg(target_arch = "wasm32")]
                    if let Some((_, Some(id))) = &location {
                        if !id.is_empty()
                            && id.len() <= 128
                            && id.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                            })
                        {
                            startup_session.set(Some(id.clone()));
                        }
                    }
                    base_url.set(format!("{}/", address.trim_end_matches('/')));
                    connect_node();
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
                let saved = connections::startup_address(None, &history, "about:blank", false);
                if saved != "about:blank"
                    && connections::normalize_address(&saved, device_mode.read().is_mobile())
                        .is_ok()
                {
                    base_url.set(saved);
                    connect_node();
                    return;
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

    let login = move |()| {
        if node_connection.phase.read().busy() {
            return;
        }
        let draft_url = connections::normalize_address(&base_url.read(), false)
            .map(|value| format!("{value}/"));
        if draft_url.as_deref() != Ok(node_url.read().as_str()) {
            status.set("请先连接当前地址，再登录".into());
            return;
        }
        let Some(client) = shared_client.read().clone() else {
            return;
        };
        let Some(secret) = password.write().take_for(&node_url.read()) else {
            status.set("请为当前节点输入密码".into());
            return;
        };
        state::cancel_requests(connection_requests);
        node_connection.epoch += 1;
        let attempt = *node_connection.epoch.peek();
        node_connection
            .phase
            .set(connections::Phase::Authenticating);
        status.set("正在登录并加载项目…".into());
        finish_connection(client, Some(secret), attempt, node_url.read().clone());
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
                session_settings_tab.set("details".into());
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
                            session_access.set(None);
                            session_access_key.set(None);
                            session_binding.set(None);
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
        let automatic = default_session_open
            .write()
            .take()
            .is_some_and(|(epoch, id)| epoch == *node_connection.epoch.peek() && id == session_id);
        // Automatic background opening must not expand navigation behind settings.
        let expand_project = !automatic || active_page.peek().as_str() == "workspace";
        let _ = document::eval("window.__rpiNewFocusDispose?.();");
        state::cancel_requests(requests);
        transcript.set(Vec::new());
        blocked_history.set(None);
        file_generation += 1;
        pending_dialogs.set(Vec::new());
        dialog_busy.write().clear();
        btw_open.set(false);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        sidebar_open.set(false);
        active_page.set("workspace".to_owned());
        let generation = *session_generation.read() + 1;
        session_generation.set(generation);
        let trace_scope = session_scope_key(&node_url.read(), &session_id);
        opening_session.set(Some((trace_scope.clone(), generation)));
        history_window
            .write()
            .reset(trace_scope.clone(), generation, None, false);
        delete_confirmation.set(false);
        // Do not expose commands until this exact session observation is ready.
        selected_session.set(None);
        queue.set(None);
        runtime_state.set(None);
        projection.set(None);
        pending_dialogs.set(Vec::new());
        session_binding.set(None);
        session_access.set(None);
        session_access_key.set(None);
        let descriptor = transport_descriptor.read().clone();
        let epoch = *node_connection.epoch.peek();
        let opening_scope = trace_scope.clone();
        let opening_id = session_id.clone();
        let opening_current = move || {
            *session_generation.peek() == generation
                && *node_connection.epoch.peek() == epoch
                && opening_session.peek().as_ref() == Some(&(opening_scope.clone(), generation))
                && selected_session
                    .peek()
                    .as_ref()
                    .is_none_or(|id| id == &opening_id)
        };
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => {
                    // Creation/test/developer callbacks may return an ID before
                    // the sidebar cache knows it. Bind ALL open paths to fresh
                    // authoritative metadata, never infer project from that cache.
                    let session = client.get_session(&session_id).await;
                    if !opening_current() {
                        return;
                    }
                    let session = match session {
                        Ok(session) if session.session_id == session_id => session,
                        Ok(_) => {
                            opening_session.set(None);
                            status.set("节点会话身份不匹配；未打开。".into());
                            return;
                        }
                        Err(error) => {
                            opening_session.set(None);
                            status.set(format!("读取会话项目失败：{error}"));
                            return;
                        }
                    };
                    let project_id = session.project_id.clone();
                    if let Some(id) = project_id.as_ref() {
                        if !projects.peek().iter().any(|p| &p.id == id) {
                            let observed = client.projects().await;
                            if !opening_current() {
                                return;
                            }
                            match observed {
                                Ok(items) => projects.set(items),
                                Err(error) => {
                                    opening_session.set(None);
                                    status.set(format!("读取会话项目失败：{error}"));
                                    return;
                                }
                            }
                        }
                    }
                    {
                        let mut items = sessions.write();
                        items.retain(|s| s.session_id != session_id);
                        items.insert(0, session.clone());
                    }
                    selected_session_title.set(session.title);
                    selected_project.set(project_id.clone());
                    selected_project_name.set(None);
                    selected_project_path.set(None);
                    if let Some(id) = project_id.as_ref() {
                        if expand_project {
                            expanded_projects
                                .write()
                                .insert(project_scope_key(&node_url.peek(), id));
                        }
                        if let Some(project) = projects.peek().iter().find(|p| &p.id == id) {
                            let scope =
                                project_scope_key(&node_origin(node_url.peek().as_str()), id);
                            selected_project_name.set(Some(
                                project_aliases
                                    .peek()
                                    .get(&scope)
                                    .cloned()
                                    .unwrap_or_else(|| project.name.clone()),
                            ));
                            selected_project_path.set(Some(project.path.clone()));
                        } else {
                            opening_session.set(None);
                            status.set("节点未返回会话所属项目；未打开。".into());
                            return;
                        }
                    }
                    files.set(None);
                    file_content.set(None);
                    file_image.set(None);
                    git_status.set(None);
                    selected_project_file.set(None);
                    selected_change_file.set(None);
                    change_diff.set(None);
                    change_error.set(None);
                    // Unknown access must never fall back to node-global capabilities.
                    let observed = match client.session_access(&session_id).await {
                        Ok(access) => access,
                        Err(error) => {
                            let saved = work_core::api::NodeApi::native_saved_history(
                                &client,
                                &session_id,
                                None,
                            )
                            .await;
                            if opening_current() {
                                if let Ok(saved) = saved {
                                    if saved["executable"] == false
                                        && saved["sessionId"] == session_id
                                    {
                                        transcript.set(
                                            saved["messages"]
                                                .as_array()
                                                .cloned()
                                                .unwrap_or_default(),
                                        );
                                        selected_session.set(Some(session_id.clone()));
                                        blocked_history.set(Some(saved));
                                        // No projection/access/stream is fabricated. Execution remains disabled.
                                        projection.set(None);
                                        session_access.set(None);
                                        status.set(format!(
                                            "会话执行已阻止；已加载 native 只读历史：{error}"
                                        ));
                                    }
                                } else {
                                    status.set(format!(
                                        "读取会话能力失败：{error}；安全历史不可用。"
                                    ));
                                }
                                opening_session.set(None);
                            }
                            return;
                        }
                    };
                    if !opening_current() {
                        return;
                    }
                    let binding = client.session_agent_binding(&session_id).await;
                    if !opening_current() {
                        return;
                    }
                    match binding {
                        Ok(binding) => {
                            selected_backend.set(binding.backend.clone());
                            session_binding.set(Some(binding));
                        }
                        Err(error) => {
                            opening_session.set(None);
                            status.set(format!("读取执行后端绑定失败：{error}"));
                            return;
                        }
                    }
                    let history_supported = observed.supports_command("get_messages");
                    let protocol = observed.descriptor.identity.protocol.clone();
                    session_access.set(Some(observed.clone()));
                    session_access_key.set(Some((generation, epoch, session_id.clone())));
                    selected_session.set(Some(session_id.clone()));
                    let current_id = session_id.clone();
                    let current = move || {
                        *session_generation.peek() == generation
                            && *node_connection.epoch.peek() == epoch
                            && selected_session.peek().as_ref() == Some(&current_id)
                    };
                    let connection = client
                        .connect_session_with_access(session_id.clone(), &observed)
                        .await
                        .map(work_client::AgentConnection::Http);
                    match connection {
                        Ok(mut connection) => {
                            if !current() {
                                return;
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
                            opening_session.set(None);
                            if let Some(session) = sessions
                                .write()
                                .iter_mut()
                                .find(|session| session.session_id == session_id)
                            {
                                sync_sidebar_session(session, connection.projection());
                            }
                            session_observations.write().insert(
                                (epoch, session_id.clone()),
                                SessionObservation::from(connection.projection()),
                            );
                            projection.set(Some(connection.projection().clone()));
                            traces::restore_history(
                                traces,
                                &trace_scope,
                                connection.projection().blocks.clone(),
                            );
                            if !current() {
                                return;
                            }
                            history_window.write().reset(
                                trace_scope.clone(),
                                generation,
                                connection.message_page_info(),
                                history_supported,
                            );
                            transcript.set(connection.messages().to_vec());
                            status.set(format!("会话在线 · {} 已连接", protocol.to_uppercase()));
                            // Publish the snapshot and consume live events immediately.
                            // Optional metadata must not gate either path. Each task is
                            // scoped, and cannot overwrite a newer refresh/live result.
                            if let Some(project_id) = project_id.as_ref() {
                                if descriptor.as_ref().is_none_or(|d| d.supports("files")) {
                                    let client = client.clone();
                                    let project_id = project_id.clone();
                                    let current = current.clone();
                                    state::scoped_spawn(requests, async move {
                                        if let Ok(listing) = client.files(&project_id, "").await {
                                            if current() && files.peek().is_none() {
                                                files.set(Some(listing));
                                            }
                                        }
                                    });
                                }
                            }
                            if observed.supports_command("get_queue") {
                                let client = client.clone();
                                let session_id = session_id.clone();
                                let current = current.clone();
                                state::scoped_spawn(requests, async move {
                                    if let Ok(snapshot) = client.queue(&session_id, false).await {
                                        if current() && queue.peek().is_none() {
                                            queue.set(Some(snapshot));
                                        }
                                    }
                                });
                            }
                            if observed.supports_command("get_state") {
                                let client = client.clone();
                                let session_id = session_id.clone();
                                let current = current.clone();
                                state::scoped_spawn(requests, async move {
                                    if let Ok(state) = client.runtime_state(&session_id).await {
                                        if current() && runtime_state.peek().is_none() {
                                            runtime_state.set(Some(state));
                                        }
                                    }
                                });
                            }
                            loop {
                                if !current() {
                                    break;
                                }
                                let next = connection.next().await;
                                if !current() {
                                    break;
                                }
                                match next {
                                    Ok(Some(event)) => {
                                        traces::observe_event(traces, &trace_scope, &event.wire);
                                        output_motion.observe(&trace_scope, &event);
                                        if event.wire.event_type == "agent_settled" {
                                            stopping.write().remove(&trace_scope);
                                            resuming.write().remove(&(
                                                trace_scope.clone(),
                                                generation,
                                                *node_connection.epoch.peek(),
                                            ));
                                        }
                                        if let Some(pending) =
                                            pending_replies.write().get_mut(&trace_scope)
                                        {
                                            if activity::has_reply(pending, &event.wire) {
                                                pending.responded = true;
                                            }
                                        }
                                        if event.wire.event_type == "connection_reconnecting" {
                                            status.set(format!(
                                                "{} 会话连接中断，正在重试…",
                                                protocol.to_uppercase()
                                            ));
                                        } else if event.wire.event_type == "connection_online" {
                                            status.set(format!(
                                                "{} 会话已恢复，快照已重新校准",
                                                protocol.to_uppercase()
                                            ));
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
                                        if let Some(session) = sessions
                                            .write()
                                            .iter_mut()
                                            .find(|session| session.session_id == session_id)
                                        {
                                            sync_sidebar_session(session, connection.projection());
                                            if event.wire.event_type == "agent_settled" {
                                                session.pending_resume = event
                                                    .wire
                                                    .data
                                                    .get("pendingResume")
                                                    .and_then(Value::as_bool)
                                                    .unwrap_or(false);
                                                session.latest_settlement =
                                                    Some(event.wire.data.clone());
                                            } else if matches!(
                                                event.wire.event_type.as_str(),
                                                "agent_start" | "operation_accepted"
                                            ) {
                                                session.pending_resume = false;
                                            }
                                        }
                                        if connection.projection().connection
                                            == work_core::ConnectionState::Online
                                        {
                                            session_observations.write().insert(
                                                (epoch, session_id.clone()),
                                                SessionObservation::from(connection.projection()),
                                            );
                                        }
                                        projection.set(Some(connection.projection().clone()));
                                        if event.wire.event_type == "connection_online" {
                                            history_window.write().reset(
                                                trace_scope.clone(),
                                                generation,
                                                connection.message_page_info(),
                                                history_supported,
                                            );
                                        }
                                        transcript.set(
                                            history_window.peek().merge_tail(connection.messages()),
                                        );
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
                                            let refresh_access = observed.clone();
                                            let current = current.clone();
                                            state::scoped_spawn(requests, async move {
                                                if !current() {
                                                    return;
                                                }
                                                if refresh_access.supports_command("get_queue") {
                                                    let result = refresh_client
                                                        .queue(&refresh_session, false)
                                                        .await;
                                                    if !current() {
                                                        return;
                                                    }
                                                    if let Ok(snapshot) = result {
                                                        queue.set(Some(snapshot));
                                                    }
                                                }
                                                if refresh_access.supports_command("get_state") {
                                                    let result = refresh_client
                                                        .runtime_state(&refresh_session)
                                                        .await;
                                                    if !current() {
                                                        return;
                                                    }
                                                    if let Ok(state) = result {
                                                        runtime_state.set(Some(state));
                                                    }
                                                }
                                                // Preserve the original RPI sidebar refresh policy.
                                                if refresh_access.descriptor.identity.protocol
                                                    == "rpi"
                                                    && let Ok(items) =
                                                        refresh_client.sessions().await
                                                    && current()
                                                {
                                                    sessions.set(items);
                                                }
                                            });
                                        }
                                    }
                                    Ok(None) => {
                                        status.set(format!(
                                            "{} 会话流已关闭 · 会话状态需重新同步",
                                            protocol.to_uppercase()
                                        ));
                                        break;
                                    }
                                    Err(error) => {
                                        status.set(format!("会话流错误：{error}"));
                                        break;
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            if current() {
                                opening_session.set(None);
                                status.set(format!("打开会话失败：{error}"));
                            }
                        }
                    }
                }
                Err(error) => {
                    if opening_current() {
                        opening_session.set(None);
                        status.set(format!("地址无效：{error}"));
                    }
                }
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
        session_access.set(None);
        session_access_key.set(None);
        session_binding.set(None);
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
        file_image.set(None);
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
                    if *file_generation.read() != generation {
                        return;
                    }
                    if let Some(files_result) = files_result {
                        match files_result {
                            Ok(listing) => files.set(Some(listing)),
                            Err(error) => status.set(format!("读取项目文件失败：{error}")),
                        }
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
            // Capture the creation preference before detaching the old session. Neither
            // its persisted binding nor its scope-keyed draft is changed by this path.
            let backend = selected_backend.peek().clone();
            select_project(project_id.clone(), false);
            active_view.set("conversation".into());
            new_session_busy.set(true);
            new_session_follow.set(true);
            status.set("正在准备新会话…".into());
            let generation = *session_generation.peek();
            let epoch = *node_connection.epoch.peek();
            let origin = node_url.peek().clone();
            // A project switch must not cancel a successful POST and lose its id.
            state::scoped_spawn(submissions, async move {
                let result =
                    {
                        match client
                            .create_session_with_backend(Some(&project_id), &backend)
                            .await
                        {
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
                let follow = *new_session_follow.peek()
                    && active_page.peek().as_str() == "workspace"
                    && active_view.peek().as_str() == "conversation";
                new_session_follow.set(false);
                match result {
                    Ok(mut session) => {
                        let id = session.session_id.clone();
                        if *node_connection.epoch.peek() != epoch {
                            return;
                        }
                        // Some HTTP nodes omit projectId from their session detail
                        // response. Preserve the explicit creation context in that
                        // case instead of turning a project session into a standalone one.
                        if session.project_id.is_none() {
                            session.project_id = Some(project_id.clone());
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
                        if follow
                            && *session_generation.peek() == generation
                            && selected_project.peek().as_ref() == Some(&project_id)
                        {
                            focus_new_session.set(Some(id.clone()));
                            pending_open.set(Some((false, id)));
                            status.set("新会话已准备好，直接输入你的需求。".into());
                        }
                    }
                    Err(error) => {
                        if follow && *session_generation.peek() == generation {
                            let message = format!("创建会话失败：{error}；可以重试。");
                            new_session_error.set(Some(message.clone()));
                            status.set(message);
                        }
                    }
                }
                new_session_busy.set(false);
            });
        };

    // Shared by session settings and the resource panel. Changing an execution backend is
    // a creation preference plus an explicit independent-session creation, never
    // a setter on the selected binding. Retain the original scope-keyed draft.
    let change_backend = Callback::new(move |backend: String| {
        if *new_session_busy.peek()
            || !matches!(*node_connection.phase.peek(), connections::Phase::Online)
        {
            return;
        }
        if !backend_options
            .peek()
            .iter()
            .any(|option| option.id == backend && option.available)
        {
            return;
        }
        if selected_session.peek().is_some() && session_binding.peek().is_none() {
            return;
        }
        let project = selected_session.peek().as_ref().and_then(|id| {
            sessions
                .peek()
                .iter()
                .find(|s| &s.session_id == id)
                .and_then(|s| s.project_id.clone())
        });
        if selected_session.peek().is_some() && project.is_none() {
            status.set("当前会话没有项目，无法在同一项目创建后端会话。".into());
            return;
        }
        selected_backend.set(backend);
        if selected_session.peek().is_some() {
            new_session(project);
        }
    });

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
            // Reconnecting the retained session must restore its transport,
            // not navigate away from the node settings the user is inspecting.
            let automatic = default_session_open
                .peek()
                .as_ref()
                .is_some_and(|(epoch, target)| {
                    *epoch == *node_connection.epoch.peek() && target == &id
                });
            let retained_page = (!project
                && (automatic || selected_session.peek().as_ref() == Some(&id)))
            .then(|| active_page.peek().clone());
            if project {
                open_project(id);
            } else {
                open_session(id);
            }
            if let Some(page) = retained_page {
                active_page.set(page);
            }
        }
    });

    let mut load_file = move |path: String, reset_view: bool| {
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
        if reset_view {
            project_file_lines.set(None);
            project_file_mode.set("preview".into());
        }
        file_loading.set(true);
        file_error.set(None);
        let generation = *preview_generation.read() + 1;
        preview_generation.set(generation);
        selected_project_file.set(Some(path.clone()));
        file_content.set(None);
        file_image.set(None);
        status.set("正在加载文件内容…".into());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match if project_file_kind(&path) == "image" {
                    client.image(&project_id, &path).await.map(|image| {
                        (
                            work_client::FileContent {
                                path,
                                content: String::new(),
                            },
                            Some(message_image::project_image(image)),
                        )
                    })
                } else {
                    client
                        .file(&project_id, &path)
                        .await
                        .map(|content| (content, None))
                } {
                    Ok((content, image)) if *preview_generation.read() == generation => {
                        file_loading.set(false);
                        file_content.set(Some(content));
                        file_image.set(image);
                        status.set("文件内容已加载".into());
                    }
                    Err(error) if *preview_generation.read() == generation => {
                        file_loading.set(false);
                        file_error.set(Some(error.to_string()));
                        status.set(format!("读取文件失败：{error}"));
                    }
                    _ => {}
                },
                Err(error) if *preview_generation.read() == generation => {
                    file_loading.set(false);
                    file_error.set(Some(error.to_string()));
                    status.set(format!("地址无效：{error}"));
                }
                _ => {}
            }
        });
    };
    let mut open_file = move |path: String| load_file(path, true);

    let message_link_scope = use_memo(move || {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            node_url.read(),
            selected_project.read().as_deref().unwrap_or_default(),
            selected_session.read().as_deref().unwrap_or_default()
        )
    });
    use_context_provider(|| message_links::LinkContext {
        root: selected_project_path,
        scope: message_link_scope,
        onopen: EventHandler::new(move |(scope, file): (String, message_links::FileLink)| {
            // An old rendered message must never read from a newly selected node/project.
            if scope != *message_link_scope.peek() {
                return;
            }
            if !transport_descriptor
                .peek()
                .as_ref()
                .is_some_and(|descriptor| descriptor.supports("files"))
            {
                status.set("当前节点不支持项目文件预览".into());
                return;
            }
            let image = project_file_kind(&file.path) == "image";
            open_file(file.path);
            project_file_lines.set(if image { None } else { file.lines });
            project_file_mode.set(if image { "preview" } else { "source" }.into());
            project_files_open.set(true);
        }),
    });
    use_effect(move || {
        let _scope = message_link_scope.read();
        project_files_open.set(false);
        project_file_preview.set(false);
        project_file_lines.set(None);
        preview_generation += 1;
        file_content.set(None);
        file_image.set(None);
        file_error.set(None);
    });
    use_effect(move || {
        let lines = *project_file_lines.read();
        let loaded = file_content.read().is_some();
        if let (Some((start, _)), true) = (lines, loaded) {
            let _ = document::eval(&format!(
                "requestAnimationFrame(() => {{ const rows = document.querySelectorAll('#project-files .project-code-line'); const row = rows[Math.min({start} - 1, rows.length - 1)]; row?.scrollIntoView({{block:'center'}}); }});"
            ));
        }
    });

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

    let mut open_change = move |path: String, staged: bool| {
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
        change_staged.set(staged);
        let generation = *change_generation.read() + 1;
        change_generation.set(generation);
        selected_change_file.set(Some(path.clone()));
        change_diff.set(None);
        change_error.set(None);
        status.set("正在加载 Git patch…".into());
        state::scoped_spawn(requests, async move {
            match client_result {
                Ok(client) => match client.diff(&project_id, &path, staged).await {
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

    // Keep Git reads independent of file previews and session snapshots.
    // The generation also invalidates patches when refreshing or changing scope.
    use_effect(move || {
        let scope = (node_url.read().clone(), selected_project.read().clone());
        let client = shared_client.read().clone();
        let descriptor = transport_descriptor.read().clone();
        let reviewing = active_view.read().as_str() == "changes";
        let _refresh = *git_refresh.read();
        let scope_changed = git_scope.peek().as_ref() != Some(&scope);
        if scope_changed {
            git_scope.set(Some(scope.clone()));
            git_status.set(None);
            change_query.set(String::new());
            change_directory_filter.set(String::new());
            change_status_filter.set("all".into());
            changes_unreviewed.set(false);
        } else if !reviewing && git_status.peek().is_some() {
            return;
        }
        let generation = *git_generation.peek() + 1;
        git_generation.set(generation);
        let patch_generation = *change_generation.peek() + 1;
        change_generation.set(patch_generation);
        selected_change_file.set(None);
        change_diff.set(None);
        change_error.set(None);
        change_mobile_preview.set(false);
        reviewed_changes.write().clear();
        git_error.set(None);
        git_loading.set(false);
        let Some(project_id) = scope.1.clone() else {
            return;
        };
        let Some(client) = client else { return };
        let Some(descriptor) = descriptor else { return };
        if !descriptor.supports("git_diff") {
            git_error.set(Some("当前节点不支持 Git 变更读取。".into()));
            return;
        }
        git_loading.set(true);
        git_status.set(None);
        state::scoped_spawn(requests, async move {
            let result = client.git_status(&project_id).await;
            if *git_generation.peek() != generation
                || selected_project.peek().as_ref() != Some(&project_id)
                || *node_url.peek() != scope.0
            {
                return;
            }
            git_loading.set(false);
            match result {
                Ok(repository) => git_status.set(Some(repository)),
                Err(error) => git_error.set(Some(error.to_string())),
            }
        });
    });

    use_effect(move || {
        let key = selected_session.read().clone().map(|id| {
            (
                *session_generation.read(),
                *node_connection.epoch.read(),
                id,
            )
        });
        let observed_key = session_access_key.read().clone();
        if observed_key.is_some() && observed_key != key {
            session_access.set(None);
            session_access_key.set(None);
            // Invalidate in-flight page replies as well as future claims.
            let mut window = history_window.write();
            window.enabled = false;
            window.busy = false;
            window.revision = window.revision.saturating_add(1);
        }
    });
    let session_supports = move |command: &str| {
        let Some(id) = selected_session.read().clone() else {
            return false;
        };
        session_access_key.read().as_ref()
            == Some(&(
                *session_generation.read(),
                *node_connection.epoch.read(),
                id,
            ))
            && session_access
                .read()
                .as_ref()
                .is_some_and(|access| access.supports_command(command))
    };
    let session_dialog_reply = move || {
        let Some(id) = selected_session.read().clone() else {
            return false;
        };
        session_access_key.read().as_ref()
            == Some(&(
                *session_generation.read(),
                *node_connection.epoch.read(),
                id,
            ))
            && session_access
                .read()
                .as_ref()
                .is_some_and(|access| access.dialog_reply)
    };

    let recover_queued = move |scope: &str, text: &str| {
        let existing = state::draft_for(&drafts.peek(), scope).text;
        update_draft(
            drafts,
            scope,
            message_queue::recovered_text(&existing, text),
        );
    };
    let mut send_prompt = move |continue_prompt: bool, queued_text: Option<String>| {
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
        let from_queue = queued_text.is_some();
        let mut draft = state::draft_for(&drafts.read(), &draft_scope);
        if let Some(text) = queued_text {
            draft.text = text;
        }
        if continue_prompt {
            draft.text = "继续".into();
        }
        let Some(client) = shared_client.read().clone() else {
            status.set("请先连接执行节点；草稿已保留。".into());
            return;
        };
        if !session_supports("prompt") {
            status.set("当前会话不支持 prompt，或能力尚未确认；草稿已保留。".into());
            return;
        }
        let auto_rename = session_access
            .peek()
            .as_ref()
            .is_some_and(|access| access.descriptor.identity.protocol == "rpi");
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
        // Sending a queued row must not consume unrelated composer files or
        // clear its draft. The normal prompt/echo/settlement lifecycle is shared.
        let submission = if from_queue {
            composer_extras::AttachmentSubmission {
                text: draft.text.clone(),
                snapshot: Default::default(),
                content_blocks: None,
            }
        } else {
            match attachment_store.read().prepare(
                &scope,
                &draft.text,
                session_access
                    .read()
                    .as_ref()
                    .is_some_and(|access| access.supports_prompt_content()),
            ) {
                Ok(submission) => submission,
                Err(error) => {
                    status.set(error);
                    return;
                }
            }
        };
        if submission.text.trim().is_empty() {
            return;
        }
        let prompt_text = submission.text;
        let content_blocks = submission.content_blocks;
        let attachment_snapshot = submission.snapshot;
        pending_replies.write().insert(
            scope.clone(),
            activity::PendingReply {
                text: prompt_text.clone(),
                draft_text: Some(draft.text.clone()),
                content: content_blocks.clone(),
                users_before: activity::users(&transcript.peek()),
                user_boundary: transcript
                    .peek()
                    .iter()
                    .rev()
                    .find(|message| message["role"].as_str() == Some("user"))
                    .and_then(message_id)
                    .map(str::to_owned),
                cursor_before: projection.peek().as_ref().and_then(|run| run.cursor),
                accepted: false,
                responded: false,
            },
        );
        scroll_follow::follow_output(&scope);
        sending.write().insert(draft_scope.clone());
        let generation = *session_generation.read();
        let context = model_context();
        state::scoped_spawn(submissions, async move {
            let response = if let Some(blocks) = content_blocks {
                client.prompt_content(&session_id, &blocks).await
            } else {
                client.prompt(&session_id, &prompt_text).await
            };
            let result = reconcile_prompt_response(&client, response)
                .await
                .map(|response| {
                    response.success
                        && response
                            .data
                            .as_ref()
                            .and_then(|data| data["accepted"].as_bool())
                            != Some(false)
                });
            sending.write().remove(&draft_scope);
            if result == Ok(true) && !from_queue {
                attachment_store
                    .write()
                    .acknowledge(&scope, &attachment_snapshot);
            }
            if from_queue && result != Ok(true) {
                recover_queued(&draft_scope, &prompt_text);
                queue_errors.write().insert(
                    scope.clone(),
                    format!(
                        "发送未确认，消息已取回输入框；请核对会话后再发送。{}",
                        result.as_ref().err().map_or("", String::as_str)
                    ),
                );
            }
            if model_context() != context {
                pending_replies.write().remove(&scope);
                return;
            }
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
                    if unchanged && !from_queue {
                        update_draft(drafts, &draft_scope, String::new());
                        queue_errors.write().remove(&scope);
                    }
                    // Settlement/echo can precede the ACK. Do not recreate waiting.
                    if pending_replies
                        .peek()
                        .get(&scope)
                        .is_some_and(|pending| !pending.responded)
                    {
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
                        if auto_rename
                            && client.rename_session(&session_id, &title).await.is_ok()
                            && model_context() == context
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

    let reply_order = transcript_order.clone();
    use_effect(move || {
        let id = selected_session.read().clone();
        let Some(id) = id else {
            return;
        };
        let scope = session_scope_key(&node_url.read(), &id);
        let pending = pending_replies.read().get(&scope).cloned();
        let messages = transcript.read().clone();
        if let Some(pending) = pending.as_ref()
            && let Some(echo) = activity::echo_message(pending, &messages)
            && (pending.responded || activity::replied_in_history(pending, &messages))
        {
            // Transfer the optimistic slot before dropping the only state that
            // knows its identity; the cleanup effect may precede the next render.
            if let Some(id) = message_id(echo) {
                let order_scope = format!(
                    "{scope}::{}::{}::{}",
                    session_generation.read(),
                    history_window.read().revision,
                    projection.read().as_ref().map_or(0, |run| run.generation)
                );
                let pending_id = format!(
                    "ui-pending-user-{}-{:?}",
                    pending.users_before, pending.cursor_before
                );
                reply_order
                    .borrow_mut()
                    .rekey_history(&order_scope, &pending_id, id);
            }
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
        let Some(client) = shared_client.read().clone() else {
            status.set("请先连接执行节点；草稿已保留。".into());
            return;
        };
        if !session_supports("queue_message") {
            status.set("当前会话不支持 queue_message，或能力尚未确认；草稿已保留。".into());
            return;
        }
        let submission = match attachment_store.read().prepare(&scope, &draft.text, false) {
            Ok(submission) => submission,
            Err(error) => {
                status.set(format!("未排队：{error} 请等待当前任务结束后发送图片。"));
                return;
            }
        };
        if submission.text.trim().is_empty() {
            return;
        }
        let message = submission.text;
        let attachment_snapshot = submission.snapshot;
        let delivery = "followUp";
        let can_read_queue = session_supports("get_queue");
        let generation = *session_generation.read();
        let context = model_context();
        sending.write().insert(scope.clone());
        let pending_status = "正在投递消息，等待节点确认…";
        status.set(pending_status.into());
        state::scoped_spawn(submissions, async move {
            let result: Result<QueueMutation, String> =
                match client.queue_message(&session_id, &message, delivery).await {
                    Ok(response) => Ok(response),
                    Err(ClientError::OperationOutcomeUnknown { operation }) => {
                        reconcile_operation_value(&client, operation)
                            .await
                            .and_then(|value| {
                                serde_json::from_value(value).map_err(|_| {
                                    "队列结果格式异常；未清理附件，请核对队列。".to_owned()
                                })
                            })
                    }
                    Err(error) => Err(error.to_string()),
                };
            sending.write().remove(&scope);
            if result
                .as_ref()
                .is_ok_and(|response| response.accepted == Some(true))
            {
                attachment_store
                    .write()
                    .acknowledge(&scope, &attachment_snapshot);
            }
            if model_context() != context {
                return;
            }
            match result {
                Ok(response) if response.accepted == Some(true) => {
                    if drafts
                        .read()
                        .get(&scope)
                        .is_some_and(|current| current.revision == draft.revision)
                    {
                        update_draft(drafts, &scope, String::new());
                    }
                    // A newer draft must not prevent refreshing the accepted item.
                    if can_read_queue
                        && let Ok(snapshot) = client.queue(&session_id, false).await
                        && model_context() == context
                    {
                        queue_revision += 1;
                        queue.set(Some(snapshot));
                    }
                    // Enqueue success is already reflected by the queue. Clear
                    // only our pending text, not a newer connection/error status.
                    if *session_generation.read() == generation
                        && status.peek().as_str() == pending_status
                    {
                        status.set(String::new());
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

    let composer_cancelling = move || {
        projection
            .read()
            .as_ref()
            .is_some_and(|run| run.phase == RunPhase::Cancelling)
            || selected_session.read().as_ref().is_some_and(|id| {
                stopping
                    .read()
                    .contains(&session_scope_key(&node_url.read(), id))
            })
    };

    // Shared routing predicate: images must not silently become text-only queue entries.
    let composer_can_queue = move || {
        let awaiting_reply = selected_session
            .read()
            .as_ref()
            .and_then(|id| {
                pending_replies
                    .read()
                    .get(&session_scope_key(&node_url.read(), id))
                    .cloned()
            })
            .is_some_and(|pending| pending.accepted && !pending.responded)
            || resume_key().as_ref().is_some_and(|key| {
                resuming
                    .read()
                    .get(key)
                    .is_some_and(|attempt| attempt.accepted)
            });
        interaction_state(
            projection.read().as_ref(),
            transport_online(projection.read().as_ref(), &node_connection.phase.read()),
            selected_session.read().as_ref().is_some_and(|id| {
                sending
                    .read()
                    .contains(&session_scope_key(&node_url.read(), id))
            }),
            awaiting_reply,
        )
        .can_queue()
    };

    let resume_supported = move || session_supports("resume_pending");
    // Only changes in button semantics invalidate App. Text itself is subscribed
    // by ComposerInput, so typing does not regenerate the entire conversation.
    // This memo must precede continue_action: that closure also runs in render.
    let composer_flags = use_memo(move || {
        let session = selected_session.read();
        let scope = session
            .as_ref()
            .map(|id| session_scope_key(&node_url.read(), id));
        let store = drafts.read();
        let text = scope
            .as_ref()
            .and_then(|scope| store.get(scope))
            .map_or("", |draft| draft.text.as_str());
        let original = scope.as_ref().is_some_and(|scope| {
            sending.read().contains(scope)
                && pending_replies.read().get(scope).is_some_and(|pending| {
                    pending.draft_text.as_deref().unwrap_or(&pending.text) == text
                })
        });
        let has_attachments = scope.as_ref().is_some_and(|scope| {
            let attachments = attachment_store.read().snapshot(scope);
            attachments.failed || !attachments.files.is_empty()
        });
        (text.trim().is_empty() && !has_attachments, original)
    });
    // Settings needs only the boolean, never a subscription to draft contents.
    let has_pending_draft = use_memo(move || {
        drafts
            .read()
            .values()
            .any(|draft| !draft.text.trim().is_empty())
    });
    let continue_action = move || {
        let id = selected_session.read().clone()?;
        let scope = session_scope_key(&node_url.read(), &id);
        if !pending_dialogs.read().is_empty()
            || sending.read().contains(&scope)
            || pending_replies
                .read()
                .get(&scope)
                .is_some_and(|pending| !pending.responded)
            || resume_key()
                .as_ref()
                .is_some_and(|key| resuming.read().contains_key(key))
        {
            return None;
        }
        let empty = composer_flags.read().0;
        let attachments = attachment_store.read().snapshot(&scope);
        continuation::action(
            projection.read().as_ref(),
            sessions
                .read()
                .iter()
                .find(|session| session.session_id == id),
            empty,
            attachments.loading || attachments.failed || !attachments.files.is_empty(),
            resume_supported(),
        )
    };
    let continue_supported = move |action: ContinueAction| {
        if action == ContinueAction::Resume {
            return resume_supported();
        }
        session_supports("prompt")
    };
    let mut resume_session = move || {
        if config_needs_verification()
            || model_pending.read().as_ref() == Some(&model_context())
            || thinking_pending.read().as_ref() == Some(&model_context())
            || !resume_supported()
            || !transport_online(projection.read().as_ref(), &node_connection.phase.read())
        {
            return;
        }
        let (Some(id), Some(client), Some(key)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
            resume_key(),
        ) else {
            return;
        };
        if resuming.read().contains_key(&key)
            || sending.read().contains(&key.0)
            || projection
                .read()
                .as_ref()
                .is_none_or(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_)))
            || !sessions
                .read()
                .iter()
                .any(|session| session.session_id == id && session.pending_resume)
        {
            return;
        }
        let context = model_context();
        resume_revision += 1;
        let ticket = *resume_revision.peek();
        resuming.write().insert(
            key.clone(),
            ResumeAttempt {
                ticket,
                accepted: false,
            },
        );
        scroll_follow::follow_output(&key.0);
        state::scoped_spawn(requests, async move {
            let result = client.resume_pending(&id).await;
            if model_context() != context {
                return;
            }
            // A settlement arriving before the ACK already consumed this attempt.
            if resuming
                .peek()
                .get(&key)
                .is_none_or(|attempt| attempt.ticket != ticket)
            {
                return;
            }
            match result {
                Ok(_) => {
                    resuming.write().insert(
                        key,
                        ResumeAttempt {
                            ticket,
                            accepted: true,
                        },
                    );
                    status.set("继续请求已接收，等待节点实际运行与结算…".into());
                }
                Err(error) => {
                    resuming.write().remove(&key);
                    status.set(format!("继续失败：{error}；可重试，草稿已保留。"));
                }
            }
        });
    };

    let queue_can_send = move || {
        projection.read().is_some()
            && transport_online(projection.read().as_ref(), &node_connection.phase.read())
            && !composer_can_queue()
            && !composer_cancelling()
            && pending_dialogs.read().is_empty()
            && resume_key()
                .as_ref()
                .is_none_or(|key| !resuming.read().contains_key(key))
            && !config_needs_verification()
            && model_pending.read().as_ref() != Some(&model_context())
            && thinking_pending.read().as_ref() != Some(&model_context())
            && session_supports("prompt")
            && session_supports("cancel_queued")
    };
    let mut queue_action = move |action: message_queue::QueueAction| {
        use message_queue::{QueueAction, QueueStatus};
        let (Some(id), Some(client)) = (
            selected_session.peek().clone(),
            shared_client.peek().clone(),
        ) else {
            return;
        };
        let scope = session_scope_key(&node_url.peek(), &id);
        if queue_busy.peek().contains(&scope)
            || !transport_online(projection.peek().as_ref(), &node_connection.phase.peek())
            || !session_supports(action.command())
        {
            return;
        }
        if let Some(item) = action.item() {
            // Check the latest catalog again instead of trusting a stale row.
            if !queue.peek().as_ref().is_some_and(|snapshot| {
                snapshot.items.iter().any(|current| {
                    current == item
                        && QueueStatus::from_wire(current.status.as_deref()).can_cancel()
                })
            }) {
                return;
            }
        }
        if matches!(action, QueueAction::Edit(_) | QueueAction::SendNow(_))
            && action
                .item()
                .is_none_or(|item| !message_queue::recoverable_text(item))
        {
            return;
        }
        if matches!(action, QueueAction::SendNow(_)) && !queue_can_send() {
            return;
        }
        let context = model_context();
        queue_busy.write().insert(scope.clone());
        queue_revision += 1;
        queue_errors.write().remove(&scope);
        state::scoped_spawn(queue_requests, async move {
            let result = async {
                match &action {
                    QueueAction::Refresh | QueueAction::History => {}
                    QueueAction::Clear => { client.clear_queue(&id).await.map_err(|error| error.to_string())?; }
                    QueueAction::Remove(item) | QueueAction::Edit(item) | QueueAction::SendNow(item) => {
                        let cancelled = client.cancel_queued(&id, &item.entry_id).await.map_err(|error| error.to_string())?;
                        if cancelled.entry_id != item.entry_id || !message_queue::cancellation_confirmed(&cancelled.outcome) {
                            return Err(format!("消息未能撤回（{}），可能已被领取；请刷新队列。", cancelled.outcome));
                        }
                        match &action {
                            QueueAction::Edit(item) => {
                                recover_queued(&scope, &item.text);
                                if model_context() == context && active_page.peek().as_str() == "workspace" {
                                    let _ = document::eval("requestAnimationFrame(() => document.querySelector('.composer textarea')?.focus());");
                                }
                            }
                            QueueAction::SendNow(item) => {
                                // Runtime or navigation may have changed while awaiting cancellation.
                                if model_context() == context && queue_can_send() {
                                    send_prompt(false, Some(item.text.clone()));
                                } else {
                                    recover_queued(&scope, &item.text);
                                    queue_errors.write().insert(scope.clone(), "会话状态已变化，消息已取回输入框，请确认后发送。".into());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok::<(), String>(())
            }.await;
            // Recovery above must happen even if refresh fails or the user has
            // switched nodes/sessions. Readback never edits another scope.
            if model_context() == context {
                match client
                    .queue(&id, matches!(action, QueueAction::History))
                    .await
                {
                    Ok(snapshot) if model_context() == context => {
                        if matches!(action, QueueAction::History) {
                            queue_history
                                .write()
                                .insert(scope.clone(), snapshot.clone());
                        }
                        queue.set(Some(snapshot));
                    }
                    Err(error) if model_context() == context => {
                        queue_errors.write().insert(
                            scope.clone(),
                            format!("刷新队列失败：{error}；请核对后再操作。"),
                        );
                    }
                    _ => {}
                }
            }
            if let Err(error) = result {
                queue_errors.write().insert(scope.clone(), error);
            }
            queue_revision += 1;
            queue_busy.write().remove(&scope);
        });
    };

    let mut dispatch_composer = move || {
        if config_needs_verification()
            || model_pending.read().as_ref() == Some(&model_context())
            || thinking_pending.read().as_ref() == Some(&model_context())
        {
            return;
        }
        if resume_key().as_ref().is_some_and(|key| {
            resuming
                .read()
                .get(key)
                .is_some_and(|attempt| !attempt.accepted)
        }) {
            return;
        }
        if let Some(action) = continue_action() {
            if !continue_supported(action) {
                status.set("当前节点不支持继续此会话；没有发送请求。".into());
                return;
            }
            if action == ContinueAction::Resume {
                resume_session();
            } else {
                send_prompt(true, None);
            }
            return;
        }
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
        if composer_cancelling() {
            status.set("正在停止，草稿与附件已保留，请等待本轮结束。".into());
            return;
        }
        if composer_can_queue() {
            deliver_composer();
        } else {
            send_prompt(false, None);
        }
    };

    // Both button and Enter validate the same scoped attachment snapshot.
    let mut send_or_command = move || {
        if config_needs_verification()
            || model_pending.read().as_ref() == Some(&model_context())
            || thinking_pending.read().as_ref() == Some(&model_context())
        {
            return;
        }
        if let Some(session) = selected_session.read().clone() {
            let scope = session_scope_key(&node_url.read(), &session);
            let attachments = attachment_store.read().snapshot(&scope);
            if attachments.loading {
                status.set("附件仍在读取，尚未发送；草稿与附件已保留，请稍后重试。".into());
                return;
            }
            if !attachments.files.is_empty()
                && state::draft_for(&drafts.read(), &scope)
                    .text
                    .trim()
                    .starts_with('/')
            {
                status.set("附件不能随斜杠命令发送；请移除附件或改为普通消息。".into());
                return;
            }
        }
        dispatch_composer();
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
        if composer_cancelling()
            || generation != expected_generation
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
        if !session_supports("abort") {
            status.set("当前会话不支持 abort，或能力尚未确认。".into());
            return;
        }
        let scope = session_scope_key(&node_url.peek(), &session_id);
        if stopping.peek().contains(&scope) {
            return;
        }
        // Stop cancels future intent, not the draft or an already accepted send.
        pending_enter.set(None);
        stopping.write().insert(scope.clone());
        let context = model_context();
        state::scoped_spawn(submissions, async move {
            let result = match client_result {
                Ok(client) => client.abort(&session_id).await.map(|_| ()),
                Err(error) => Err(error),
            };
            if model_context() != context {
                return;
            }
            // Settlement may arrive before the acknowledgement. Never
            // replace a real terminal state with a late "waiting" message.
            let still_stopping = stopping.peek().contains(&scope);
            if result.is_err() {
                stopping.write().remove(&scope);
            }
            if still_stopping {
                match result {
                    Ok(()) => status.set("停止请求已发送，等待 agent_settled…".into()),
                    Err(error) => status.set(format!("停止失败：{error}")),
                }
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
        if !session_dialog_reply() {
            status.set("当前会话不支持确认回复，或能力尚未确认。".into());
            return;
        }
        dialog_busy.write().insert(key.clone());
        dialog_errors.write().remove(&key);
        let client_result = shared_client
            .read()
            .clone()
            .ok_or_else(|| work_client::ClientError::Sse("请先连接有效节点".into()));
        let context = model_context();
        state::scoped_spawn(requests, async move {
            // The owner resolves protocol permission IDs/results, including ACP.
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
            if model_context() != context {
                return;
            }
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

    let (composer_empty, submitting_original) = *composer_flags.read();
    let submitting = selected_session.read().as_ref().is_some_and(|id| {
        sending
            .read()
            .contains(&session_scope_key(&node_url.read(), id))
    }) || resume_key().as_ref().is_some_and(|key| {
        resuming
            .read()
            .get(key)
            .is_some_and(|attempt| !attempt.accepted)
    });
    let pending_reply = selected_session.read().as_ref().and_then(|id| {
        pending_replies
            .read()
            .get(&session_scope_key(&node_url.read(), id))
            .cloned()
    });
    let waiting_reply = pending_reply
        .as_ref()
        .is_some_and(|pending| !pending.responded)
        || resume_key().as_ref().is_some_and(|key| {
            resuming
                .read()
                .get(key)
                .is_some_and(|attempt| attempt.accepted)
        });
    let interaction = interaction_state(
        projection.read().as_ref(),
        transport_online(projection.read().as_ref(), &node_connection.phase.read()),
        submitting,
        waiting_reply,
    );
    let interaction = if interaction.is_active()
        && selected_session.read().as_ref().is_some_and(|id| {
            stopping
                .read()
                .contains(&session_scope_key(&node_url.read(), id))
        }) {
        InteractionState::Cancelling
    } else {
        interaction
    };
    let extension_locked = extension_config_locked(
        projection.read().as_ref(),
        interaction,
        !pending_dialogs.read().is_empty(),
    );
    let model_switching = model_pending.read().as_ref() == Some(&model_context());
    let thinking_switching = thinking_pending.read().as_ref() == Some(&model_context());
    let config_verification_required = config_needs_verification();
    let mut refresh_model_config = move || {
        if !session_supports("get_state") {
            model_catalog_error.set(Some("当前会话不支持状态回读，或能力尚未确认".into()));
            return;
        }
        let context = model_context();
        if model_pending.read().as_ref() == Some(&context)
            || thinking_pending.read().as_ref() == Some(&context)
        {
            return;
        }
        model_catalog_revision += 1;
        let revision = *model_catalog_revision.peek();
        let commit = *model_commit_revision.peek();
        let thinking_commit = *thinking_commit_revision.peek();
        let (Some(id), Some(client)) = (
            selected_session.read().clone(),
            shared_client.read().clone(),
        ) else {
            return;
        };
        state::scoped_spawn(requests, async move {
            let result = config_requests::bounded("核对当前配置", client.runtime_state(&id)).await;
            if model_context() != context
                || *model_catalog_revision.read() != revision
                || *model_commit_revision.read() != commit
                || *thinking_commit_revision.read() != thinking_commit
                || model_pending.read().as_ref() == Some(&context)
                || thinking_pending.read().as_ref() == Some(&context)
            {
                return;
            }
            match result {
                Ok(state) => {
                    if config_unconfirmed
                        .peek()
                        .as_ref()
                        .is_some_and(|(scope, field)| {
                            scope == &context && !field.reported_by(&state)
                        })
                    {
                        let message = "节点仍未报告待核对的配置，请刷新后重试".to_owned();
                        model_catalog_error.set(Some(message.clone()));
                        thinking_error.set(Some((context, message)));
                        return;
                    }
                    runtime_state.set(Some(state));
                    if config_needs_verification() {
                        status.set("配置状态已核对，可以继续操作".into());
                    }
                    config_unconfirmed.set(None);
                    thinking_error.set(None);
                }
                Err(error) => {
                    let message = format!("回读当前配置失败：{error}");
                    model_catalog_error.set(Some(message.clone()));
                    thinking_error.set(Some((context, message)));
                }
            }
        });
    };
    let running = interaction.is_active();
    let activity_label = interaction.activity_label(waiting_reply);
    let composer_loading = interaction == InteractionState::Cancelling
        || interaction.composer_loading(composer_empty, submitting_original);
    let composer_stop = interaction.primary_action_is_stop();
    let composer_continue = (!submitting && interaction == InteractionState::Ready)
        .then(continue_action)
        .flatten();
    let continuation_disabled = composer_continue.is_some_and(|action| !continue_supported(action));
    let sidebar_activity = |session: &SessionSummary| {
        use management::SidebarActivity;
        let scope = session_scope_key(&node_url.read(), &session.session_id);
        if opening_session
            .read()
            .as_ref()
            .is_some_and(|(key, generation)| {
                key == &scope && *generation == *session_generation.read()
            })
        {
            return Some(SidebarActivity::Loading);
        }
        if selected_session.read().as_ref() == Some(&session.session_id) {
            // Preserve authoritative failure/resume badges in a settled conversation.
            if interaction == InteractionState::Ready
                && (session.error.is_some() || session.pending_resume || session.status == "failed")
            {
                return None;
            }
            return Some(interaction.sidebar_activity(waiting_reply));
        }
        if stopping.read().contains(&scope) {
            return Some(SidebarActivity::Cancelling);
        }
        if sending.read().contains(&scope) {
            return Some(SidebarActivity::Sending);
        }
        if pending_replies
            .read()
            .get(&scope)
            .is_some_and(|pending| !pending.responded)
        {
            return Some(SidebarActivity::WaitingReply);
        }
        None
    };
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
        .cloned()
        .collect::<Vec<_>>();
    let switcher_states = visible_sessions
        .iter()
        .map(|session| {
            let observations = session_observations.read();
            let observed =
                observations.get(&(*node_connection.epoch.read(), session.session_id.clone()));
            let mut activity = sidebar_activity(session);
            if activity == Some(management::SidebarActivity::WaitingReply)
                && let Some(pending) = pending_replies
                    .read()
                    .get(&session_scope_key(&node_url.read(), &session.session_id))
                && !super::runtime::switcher_waiting_reply(session, pending, observed)
            {
                activity = None;
            }
            let state = super::runtime::switcher_session_state(session, activity, observed);
            (
                session.session_id.clone(),
                (state.0.to_owned(), state.1.to_owned()),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let repository_snapshot = git_status.read().clone();
    let change_count = repository_snapshot
        .as_ref()
        .map(|repo| repo.files.len().to_string())
        .unwrap_or_else(|| "—".into());
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
    let (workspace_kind, workspace_label, workspace_icon) = node_info::workspace_location(
        node_info::is_local(shared_client.read().as_ref(), &node_url.read()),
        &node_url.read(),
    );
    let workspace_address = node_info::safe_address(&node_url.read());
    let inspector_protocol = session_access.read().as_ref().map_or_else(
        || "会话协议尚未确认".into(),
        |access| access.descriptor.identity.protocol.clone(),
    );
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
    let composer_provider = runtime_state
        .read()
        .as_ref()
        .and_then(|runtime| runtime.provider.clone())
        .unwrap_or_default();
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
    // A complete message must take over its live slot, not jump ahead of older
    // projection-only replies. Pagination inserts before surviving history anchors.
    let order_scope = format!(
        "{}::{}::{}::{}",
        selected_session
            .read()
            .as_ref()
            .map(|id| session_scope_key(&node_url.read(), id))
            .unwrap_or_default(),
        session_generation.read(),
        history_window.read().revision,
        projection.read().as_ref().map_or(0, |run| run.generation),
    );
    // The optimistic user message also owns a slot before any early response;
    // it is display-only and never enters the persisted transcript.
    let mut display_history = transcript.read().clone();
    let pending_index = pending_reply.as_ref().and_then(|pending| {
        let pending_id = format!(
            "ui-pending-user-{}-{:?}",
            pending.users_before, pending.cursor_before
        );
        let echo = activity::echo_message(pending, &display_history);
        if let Some(echo) = echo {
            if let Some(id) = message_id(echo) {
                transcript_order
                    .borrow_mut()
                    .rekey_history(&order_scope, &pending_id, id);
            }
            None
        } else {
            let index = display_history.len();
            display_history.push(serde_json::json!({
                "entryId": pending_id, "role": "user", "content": pending.content.clone().unwrap_or_else(|| Value::String(pending.text.clone())),
                "__rpiPendingUser": true,
            }));
            Some(index)
        }
    });
    let ordered_rows = transcript_order.borrow_mut().update(
        &order_scope,
        &display_history,
        projection
            .read()
            .as_ref()
            .map_or(&[], |run| run.blocks.as_slice()),
    );

    // Keep the keyed element directly at the loop root. An expression wrapping
    // another rsx! would leave the outer list unkeyed and remount reading anchors.
    let display_rows: Vec<_> = ordered_rows
        .into_iter()
        .map(|row| {
            let pending =
                matches!(row.source, RowSource::History(index) if Some(index) == pending_index);
            let (message, block, user_key) = match row.source {
                RowSource::History(index) => {
                    let message = display_history.get(index).cloned();
                    let user_key = message
                        .as_ref()
                        .filter(|_| !pending)
                        .and_then(|message| user_timeline::target_key(message, index));
                    (message, None, user_key)
                }
                RowSource::Projection(index) => (
                    None,
                    projection
                        .read()
                        .as_ref()
                        .and_then(|run| run.blocks.get(index))
                        .cloned(),
                    None,
                ),
            };
            let entry = message
                .as_ref()
                .filter(|_| !pending)
                .and_then(message_id)
                .map(str::to_owned);
            (row.key, message, block, user_key, entry, pending)
        })
        .collect();

    let header_menu_actions = move || {
        rsx! {
            if workspace_page {
                button { role: "menuitem", disabled: selected_session.read().is_none(), onclick: move |_| { session_settings_tab.set("details".into()); session_details_open.set(true); }, Icon { name: "settings" } "会话设置" }
            }
            button { role: "menuitem", onclick: move |_| appearance_open.set(true), Icon { name: "sparkles" } "外观与主题" }
            if workspace_page && active_view.read().as_str() == "changes" {
                button { role: "menuitem", disabled: *git_loading.read() || selected_project.read().is_none(), onclick: move |_| git_refresh += 1,
                    Icon { name: "refresh" } if *git_loading.read() { "刷新中…" } else { "刷新变更" }
                }
            }
            button { role: "menuitem", disabled: !matches!(*node_connection.phase.read(), connections::Phase::Online) || catalog_refresh.read().busy,
                onclick: move |_| {
                    if !matches!(*node_connection.phase.peek(), connections::Phase::Online) || catalog_refresh.peek().busy { return; }
                    let mut refresh = catalog_refresh.write();
                    refresh.epoch = *node_connection.epoch.peek();
                    refresh.request = refresh.request.saturating_add(1);
                    refresh.busy = true;
                    refresh.message.clear();
                }, Icon { name: "refresh" } "刷新会话"
            }
            if let Some(session_id) = selected_session.read().as_ref() {
                if development_associations.read().contains_key(&session_scope_key(&node_origin(node_url.read().as_str()), session_id)) {
                    button { role: "menuitem", onclick: move |_| active_page.set("meta-agent".into()), "回到元Agent源稿" }
                }
            }
            div { class: "header-menu-connection", role: "img", aria_label: "{node_connection_label}",
                Icon { name: if connection_tone == "online" { "server" } else { "info" } } "{node_connection_label}"
            }
            hr {}
        }
    };

    rsx! {
        document::Link { rel: "icon", r#type: "image/svg+xml", href: crate::shared::brand::icon_url() }
        if matches!(*node_connection.phase.read(), connections::Phase::Online) {
            super::workspace_sync::WorkspaceSync {
                key: "{node_url.read()}::{node_connection.epoch.read()}",
                address: node_url.read().clone(),
                epoch: *node_connection.epoch.read(),
                client: shared_client,
                connection_epoch: node_connection.epoch,
                sessions, projects, selected_session, selected_session_title,
                selected_project, selected_project_name, project_aliases,
                refresh: catalog_refresh,
            }
        }
        updates::UpdateNotice { onopen: move |()| { active_page.set("settings".to_owned()); settings_section.set("updates".to_owned()); } }
        settings::DesktopPairing {
            shared_client, base_url, auth_state,
            view: format!("{}:{}", active_page.read(), settings_section.read()),
        }
            style { "{THEMES}" }
            style { "{CSS}" }
            style { "{RESOURCES_CSS}" }
            style { "{META_AGENT_CSS}" }
            style { {node_info::CSS} }
            style { "{PROJECT_PICKER_CSS}" }
            style { "{TRACES_CSS}" }
            style { "{CONVERSATION_CSS}" }
            style { {model_picker::CSS} }
            style { {turn_timing::CSS} }
            style { {user_timeline::CSS} }
            style { "{EXTRAS_CSS}" }
            style { "{QUEUE_CSS}" }
            style { {execution_backend::CSS} }
            style { {management::CSS} }
            style { "{REVIEW_CSS}" }
            style { {activity::CSS} }
            style { "{runtime_stats::CSS}" }
            runtime_stats::RuntimeDialogs {
                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                runtime_state, projection, transcript, last_turn: last_turn_usage,
                shared_client, selected_session, session_generation, compact_revision, compact_pending, node_compacting,
                compact_result, pending_ask: !pending_dialogs.read().is_empty(), stats_open, context_open,
                ontools: move |()| session_details_open.set(true),
            }
            if let Some((address, epoch, project_id)) = node_info_target.read().clone() {
                if address == *node_url.read() && epoch == *node_connection.epoch.read() {
                    {
                        let project = project_id.as_ref().and_then(|id| projects.read().iter().find(|project| &project.id == id).cloned());
                        let project_name = project.as_ref().map(|project| project_aliases.read().get(&project_scope_key(&node_origin(&address), &project.id)).cloned().unwrap_or_else(|| project.name.clone())).unwrap_or_default();
                        rsx! {
                            node_info::NodeInfoDialog {
                                project, project_name, local: node_info::is_local(shared_client.read().as_ref(), &address), address,
                                phase: node_connection.phase.read().clone(), probe: node_probe.read().clone(),
                                descriptor: transport_descriptor.read().clone(), capabilities: capabilities.read().clone(),
                                session_connection: projection.read().as_ref().map(|run| match run.connection {
                                    work_core::ConnectionState::Online => "online",
                                    work_core::ConnectionState::Reconnecting | work_core::ConnectionState::Resyncing => "connecting",
                                    work_core::ConnectionState::Offline => "offline",
                                }.to_owned()),
                                onrefresh: refresh_node_probe,
                                onsettings: move |()| { node_info_target.set(None); active_page.set("settings".into()); settings_section.set("devices".into()); sidebar_open.set(false); },
                                onclose: move |()| node_info_target.set(None),
                            }
                        }
                    }
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
                aside { id: "project-sidebar", class: if *sidebar_open.read() { "sidebar open" } else { "sidebar" },
                    a { class: "brand", href: "#",
                        BrandMark { class: "brand-mark" }
                        strong { "rpi" span { "work" } }
                        span { class: "version", "MVP" }
                    }
                    node_info::WorkspaceLocation {
                        local: node_info::is_local(shared_client.read().as_ref(), &node_url.read()),
                        address: node_url.read().clone(),
                        onsettings: move |()| { active_page.set("settings".into()); settings_section.set("devices".into()); sidebar_open.set(false); },
                    }
                    button { class: "new-task", disabled: *new_session_busy.read(), onclick: move |_| new_session(None), Icon { name: "plus" } if *new_session_busy.read() { "正在创建…" } else { "新建任务" } }
                    button { class: "search-trigger", onclick: move |_| { global_search_open.set(true); },
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
                    div { class: "section-label", "项目"
                        div { class: "sidebar-catalog-actions",
                            button { aria_label: "刷新会话", title: "从当前节点拉取最新会话", aria_busy: catalog_refresh.read().busy,
                                disabled: !matches!(*node_connection.phase.read(), connections::Phase::Online) || catalog_refresh.read().busy,
                                onclick: move |_| {
                                    if !matches!(*node_connection.phase.peek(), connections::Phase::Online) || catalog_refresh.peek().busy { return; }
                                    let mut refresh = catalog_refresh.write();
                                    refresh.epoch = *node_connection.epoch.peek();
                                    refresh.request = refresh.request.saturating_add(1);
                                    refresh.busy = true;
                                    refresh.message.clear();
                                }, Icon { name: "refresh" }
                            }
                            button { aria_label: "添加项目", title: "添加项目", onclick: move |_| {
                                if matches!(*node_connection.phase.read(), connections::Phase::Online) {
                                    project_error.set(None); show_project_form.set(true);
                                } else { open_node_settings(); }
                            }, Icon { name: "plus" } }
                        }
                    }
                    if catalog_refresh.read().epoch == *node_connection.epoch.read() && (catalog_refresh.read().busy || !catalog_refresh.read().message.is_empty()) {
                        p { class: "sidebar-catalog-status", role: "status", aria_live: "polite",
                            if catalog_refresh.read().busy { "正在拉取最新会话…" } else { "{catalog_refresh.read().message}" }
                        }
                    }
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
                            strong { "添加你的第一个项目" }
                            span { "选择工作目录，开始一个会话。" }
                            button { class: "button", onclick: move |_| {
                                if matches!(*node_connection.phase.read(), connections::Phase::Online) { project_error.set(None); show_project_form.set(true); } else { open_node_settings(); }
                            }, "添加项目" }
                        }
                    } else {
                        if visible_sessions.is_empty() && !catalog_refresh.read().busy {
                            div { class: "sidebar-empty session-catalog-empty", role: "status",
                                Icon { name: "message" }
                                strong { "当前节点还没有活动会话" }
                                span { "创建后，会话会按项目显示在这里；已归档会话可从底部查看。" }
                                button { class: "button", disabled: *new_session_busy.read() || !matches!(*node_connection.phase.read(), connections::Phase::Online), onclick: move |_| new_session(None),
                                    if *new_session_busy.read() { "正在创建…" } else { "新建会话" }
                                }
                            }
                        }
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
                                    node_info::NodeBadge {
                                        local: node_info::is_local(shared_client.read().as_ref(), &node_url.read()),
                                        tone: node_info::status(&node_connection.phase.read(), &node_probe.read()).0.to_owned(),
                                        label: node_info::status(&node_connection.phase.read(), &node_probe.read()).1.to_owned(),
                                        project_name: display_name.clone(),
                                        onopen: { let id=project_id.clone(); move |()| show_node_info(Some(id.clone())) },
                                        oncontext: { let menu=format!("project-menu-{project_id}"); move |()| management::open_menu(&menu) },
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
                                        button { role: "menuitem", onclick: { let id=project_id.clone(); move |_| show_node_info(Some(id.clone())) }, "节点信息" }
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
                                                                activity: sidebar_activity(session),
                                                                scope: session_scope_key(&node_url.read(), &id),
                                                                busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &id)),
                                                                now: *sidebar_now.read(), onopen: open_session, onaction: session_action,
                                                                onnodeinfo: { let project_id=session.project_id.clone(); move |()| show_node_info(project_id.clone()) },
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
                                                span { class: "sidebar-empty", "还没有会话，点击项目旁的 + 开始" }
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
                                            activity: sidebar_activity(session),
                                            scope: session_scope_key(&node_url.read(), &id),
                                            busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &id)),
                                            now: *sidebar_now.read(), onopen: open_session, onaction: session_action,
                                            onnodeinfo: { let project_id=session.project_id.clone(); move |()| show_node_info(project_id.clone()) },
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
                super::sidebar_resize::Handle {}
                div { class: "main-column",
                    header { class: if workspace_page { "app-header has-session-controls" } else { "app-header" },
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
                                    if let Some(title) = selected_session_title.read().as_deref().filter(|s| !s.trim().is_empty()) { "{title}" } else if selected_session.read().is_some() { "未命名会话" } else { "未选择会话" }
                                } else { "{navigation_title}" }
                            }
                        }
                        if workspace_page {
                            button { class: "quick-session", aria_label: "切换会话", onclick: move |_| { session_switcher_open.set(true); },
                                span {
                                    small { "{selected_project_name.read().as_deref().unwrap_or(\"rpi-work\")}" }
                                    strong { {selected_session_title.read().as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(if selected_session.read().is_some() { "未命名会话" } else { "选择一个会话" }).to_owned()} }
                                }
                                Icon { name: "chevron".to_owned() }
                            }
                        }
                        div { class: "top-actions",
                            if workspace_page && active_view.read().as_str() == "conversation" {
                                button { class: "icon-button timeline-toggle", r#type: "button",
                                    aria_label: if timeline_visible() { "隐藏时间线" } else { "显示时间线" },
                                    title: if timeline_visible() { "隐藏时间线" } else { "显示时间线" },
                                    aria_pressed: timeline_visible(), aria_expanded: timeline_visible(),
                                    disabled: selected_session.read().is_none() || active_view.read().as_str() != "conversation",
                                    onclick: move |_| timeline_visible.set(!timeline_visible()),
                                    Icon { name: "clock" }
                                }
                            }
                            span { class: "status-badge {task_tone}", role: "img", aria_label: "{task_label}", title: "{task_label}",
                                i { "aria-hidden": "true" }
                                span { class: "header-status-icon", Icon { name: match task_label {
                                    "生成文本" => "edit",
                                    "执行工具" => "tool",
                                    _ => match task_tone {
                                        "ready" | "completed" => "check",
                                        "running" | "cancelling" => "clock",
                                        "awaiting-input" => "ask",
                                        "interrupted" => "close",
                                        _ => "info",
                                    },
                                } } }
                                span { class: "header-status-label", "{task_label}" }
                            }
                            button { class: "context-toggle", aria_label: "查看任务上下文", onclick: move |_| {
                                let current = *inspector_open.read();
                                inspector_open.set(!current);
                            }, Icon { name: "panel".to_owned() } }
                            if let Some(session)=sessions.read().iter().find(|session| selected_session.read().as_deref() == Some(session.session_id.as_str())) {
                                management::SessionMenu { id: session.session_id.clone(), title: session.title.clone().unwrap_or_default(), archived: session.archived, pinned: session.pinned,
                                    prefix: "header".to_owned(), extra_actions: Some(header_menu_actions()), busy: session_action_busy.read().contains(&session_scope_key(&node_url.read(), &session.session_id)), onaction: session_action }
                            } else {
                                button { class: "header-more icon-button", aria_label: "更多操作", aria_haspopup: "menu", "popovertarget": "header-workspace-menu", Icon { name: "more" } }
                                div { id: "header-workspace-menu", class: "workspace-menu", popover: "auto", role: "menu", aria_label: "更多操作", {header_menu_actions()} }
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
                    div { class: "page-view resource-center-page",
                        crate::features::resources::center::ResourceCenter {
                            key: "resource-center-{active_page}-{node_url}-{selected_project:?}-{node_connection.epoch}-{session_generation}-{selected_session:?}",
                            kind: active_page.read().clone(), project: selected_project.read().clone(),
                            session: selected_session.read().clone(), session_name: selected_session_title.read().clone(),
                            session_scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)),
                            library: resource_library, shared_client, connection_epoch: node_connection.epoch,
                            session_generation, idle: !extension_locked,
                            online: matches!(*node_connection.phase.read(), connections::Phase::Online),
                            onopen: move |id: String| { active_page.set("workspace".into()); pending_open.set(Some((false,id))); },
                            onconfigure: move |()| { active_page.set("workspace".into()); session_settings_tab.set("design".into()); session_details_open.set(true); },
                            onback: move |()| active_page.set("workspace".into()),
                        }
                    }
                }
                if active_page.read().as_str() == "meta-agent" {
                    if let (Some(project_id), Some(project_name)) = (selected_project.read().clone(), selected_project_name.read().clone()) {
                        if project_purposes
                            .read()
                            .get(&project_scope_key(&node_origin(node_url.read().as_str()), &project_id))
                            .is_some_and(|purpose| purpose == "元Agent") {
                            meta_agent::node::NodeAuthoring {
                                key: "authoring-{node_url}-{project_id}-{node_connection.epoch}",
                                project: project_id.clone(), shared_client, connection_epoch: node_connection.epoch,
                                legacy: meta_store,
                                onopen: move |id: String| { active_page.set("workspace".into()); pending_open.set(Some((false,id))); },
                            }
                            details {
                                summary { "旧版惰性设计工具（仅内存预览；不再写回旧存储）" }
                                p { "这里的保存/结构快照/设计发布仅修改本次内存或惰性设计库，不是节点源码、native 测试或实际安装。持久开发请使用上方节点流程；原始旧字节可显式导入。" }
                                meta_agent::MetaAgentPage {
                                    project_id, project_name, store: meta_store,
                                    ondevelop: move |_| status.set("请在上方节点源码开发区显式导入并创建真实开发会话；旧设计不授予执行权限。".into()),
                                    onpublish: Callback::new(move |value: Value| resources::import_design_package(&mut resource_library.write(), &value)),
                                }
                            }
                        } else { div { class: "page-view", p { "请选择元Agent项目。" } } }
                    } else { div { class: "page-view", p { "请先选择元Agent项目。" } } }
                }
                if active_page.read().as_str() == "settings" {
                    SettingsPage { shared_client, requests, active_page, settings_section, base_url, status, auth_state, password, runtime_state, capabilities, selected_session, session_generation,
                        connection_epoch: node_connection.epoch, online: matches!(*node_connection.phase.read(), connections::Phase::Online),
                        mobile_client: device_mode.read().is_mobile(),
                        update_busy: !sending.read().is_empty() || projection.read().as_ref().is_some_and(|run| !matches!(run.phase, RunPhase::Ready | RunPhase::Settled(_))) || *has_pending_draft.read() || attachment_store.read().has_pending_content() || btw_store.read().has_pending_content(),
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
                        if let Some(saved) = blocked_history.read().clone() {
                            div { role: "status", class: "resource-boundary",
                                p { "只读 native 历史 · execution disabled · blocked operation: {saved[\"blockedOperationId\"]}。请在会话资源面板恢复原操作或明确修正未提交的 blocked 配置。" }
                                if !saved["cursor"].is_null() {
                                    button { onclick: move |_| {
                                        let (Some(client),Some(id)) = (shared_client.peek().clone(),selected_session.peek().clone()) else { return; };
                                        let cursor = saved["cursor"].to_string(); let epoch = *node_connection.epoch.peek(); let generation = *session_generation.peek();
                                        state::scoped_spawn(requests, async move {
                                            let result = work_core::api::NodeApi::native_saved_history(&client,&id,Some(cursor)).await;
                                            if *node_connection.epoch.peek()!=epoch || *session_generation.peek()!=generation { return; }
                                            match result { Ok(value) => { transcript.write().extend(value["messages"].as_array().cloned().unwrap_or_default()); blocked_history.set(Some(value)); }, Err(e)=>status.set(format!("历史读取冲突/失败；请重新打开并回读：{e}")) }
                                        });
                                    }, "读取后续只读历史" }
                                }
                            }
                        }
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
                                button { class: "tab-resource", aria_label: "打开会话资源配置", aria_haspopup: "dialog",
                                    disabled: selected_session.read().is_none(),
                                    onclick: move |_| { session_settings_tab.set("design".into()); session_details_open.set(true); },
                                    Icon { name: "blocks" } span { class: "resource-label", "设计引用" }
                                }
                            }
                        }
                        if active_view.read().as_str() == "changes" {
                            div { id: "changes-panel", role: "tabpanel", aria_busy: *git_loading.read(), aria_labelledby: "changes-tab", class: if *change_mobile_preview.read() { "change-workspace show-diff" } else { "change-workspace" },
                                div { class: "review-overview",
                                    div { class: "review-heading",
                                        h2 { "变更" span { class: "review-total", "{change_count}" } }
                                        if let Some(branch) = repository_snapshot.as_ref().and_then(|repo| repo.branch.as_ref()) {
                                            p { class: "review-branch", title: "{branch}", "{branch}" }
                                        }
                                    }
                                    div { class: "review-actions",
                                        span { class: "review-progress", title: "仅记录本次阅读进度，不代表 Git 暂存或提交；刷新后重置。", "{reviewed_count}/{change_count} 已审阅" }
                                    }
                                }
                                div { class: "change-layout",
                                    aside { class: "change-browser",
                                        div { class: "change-filters",
                                            div { class: "change-search", Icon { name: "search".to_owned() } input { aria_label: "搜索变更文件", placeholder: "搜索文件…", value: "{change_query}", oninput: move |event| change_query.set(event.value()) } }
                                            div { class: "change-filter-row",
                                                input { aria_label: "筛选目录", placeholder: "所有目录", value: "{change_directory_filter}", oninput: move |event| change_directory_filter.set(event.value()) }
                                                select { aria_label: "筛选变更类型", value: "{change_status_filter}", onchange: move |event| change_status_filter.set(event.value()),
                                                    option { value: "all", "全部类型" }
                                                    option { value: "M", "已修改 M" }
                                                    option { value: "A", "新增 A" }
                                                    option { value: "D", "删除 D" }
                                                    option { value: "R", "重命名 R" }
                                                    option { value: "C", "复制 C" }
                                                    option { value: "U", "冲突 U" }
                                                    option { value: "T", "类型变化 T" }
                                                }
                                            }
                                            div { class: "change-filter-footer",
                                                label { input { r#type: "checkbox", checked: *changes_unreviewed.read(), onchange: move |event| changes_unreviewed.set(event.checked()) } "仅未审阅" }
                                                button { class: "change-clear-filters", disabled: change_query.read().is_empty() && change_directory_filter.read().is_empty() && change_status_filter.read().as_str() == "all" && !*changes_unreviewed.read(), onclick: move |_| {
                                                    change_query.set(String::new());
                                                    change_directory_filter.set(String::new());
                                                    change_status_filter.set("all".into());
                                                    changes_unreviewed.set(false);
                                                }, "清除筛选" }
                                            }
                                        }
                                        div { class: "change-list-caption", span { "文件" } span { "{visible_changes.len()} / {change_count}" } }
                                        if *git_loading.read() {
                                            div { class: "change-no-results", role: "status", "正在读取 Git 状态…" }
                                        } else if let Some(error) = git_error.read().as_ref() {
                                            div { class: "change-no-results error-text", role: "alert", strong { "Git 状态读取失败" } p { "{error}" }
                                                button { onclick: move |_| git_refresh += 1, "重试读取变更" }
                                            }
                                        } else if git_status.read().is_none() {
                                            div { class: "change-no-results", "连接项目后加载 Git 状态。" }
                                        } else if repository_snapshot.as_ref().is_some_and(|repo| !repo.is_repository) {
                                            div { class: "change-no-results", "当前项目不是 Git 仓库。" }
                                        } else if repository_snapshot.as_ref().is_some_and(|repo| repo.files.is_empty()) {
                                            div { class: "change-no-results", "工作区干净，没有变更。" }
                                        } else if visible_changes.is_empty() {
                                            div { class: "change-no-results", strong { "没有匹配的变更" } p { "试试其他路径、类型或审阅筛选。" } }
                                        } else {
                                            div { class: "change-file-list",
                                                for changed in visible_changes.iter() {
                                                    {
                                                        let path = changed.path.clone();
                                                        let status_code = change_status_label(&changed.status);
                                                        let (directory, filename) = changed.path.rsplit_once(['/', '\\']).unwrap_or(("", &changed.path));
                                                        let selected = selected_change_file.read().as_ref() == Some(&path);
                                                        let staged = changed.staged && !changed.unstaged;
                                                        let reviewed = reviewed_changes.read().contains(&path);
                                                        rsx! {
                                                            button {
                                                                key: "review-file-{path}",
                                                                "data-change-path": "{path}", title: "{path}", aria_pressed: selected,
                                                                class: if selected { "change-file selected" } else { "change-file" },
                                                                onclick: move |_| open_change(path.clone(), staged),
                                                                Icon { name: "file".to_owned() }
                                                                span { class: "change-file-name",
                                                                    strong { "{filename}" }
                                                                    small { class: "change-file-context",
                                                                        if !directory.is_empty() { span { class: "change-file-directory", "{directory}" } span { "·" } }
                                                                        span { class: "change-file-stage", if changed.staged && changed.unstaged { "已暂存 + 工作区变更" } else if changed.staged { "已暂存" } else if changed.status == "??" { "未追踪" } else { "工作区变更" } }
                                                                    }
                                                                    if let Some(original) = changed.original_path.as_ref() { small { class: "change-file-origin", title: "原路径：{original}", "← {original}" } }
                                                                }
                                                                span { class: "file-change-status status-{status_code}", title: "Git 状态：{changed.status}", "{status_code}" }
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
                                                let unstaged_path = path.clone();
                                                let staged_path = path.clone();
                                                let changed = repository_snapshot.as_ref().and_then(|repo| repo.files.iter().find(|file| file.path == path));
                                                let has_staged = changed.is_some_and(|file| file.staged);
                                                let has_unstaged = changed.is_some_and(|file| file.unstaged);
                                                rsx! {
                                                    div { class: "selected-file-heading",
                                                        button { class: "back-to-files", aria_label: "返回文件列表", onclick: move |_| change_mobile_preview.set(false), Icon { name: "back".to_owned() } span { "文件列表" } }
                                                        div { class: "review-file-title", strong { title: "{path}", "{path}" }
                                                            small { if *change_staged.read() { "HEAD → 暂存区" } else { "暂存区 → 工作区" } }
                                                        }
                                                    }
                                                    div { class: "review-file-toolbar",
                                                        div { class: "review-diff-switch", role: "group", aria_label: "Diff 基准",
                                                            button { disabled: !has_unstaged, aria_label: "工作区 diff", aria_pressed: !*change_staged.read(), onclick: move |_| open_change(unstaged_path.clone(), false), "工作区" }
                                                            button { disabled: !has_staged, aria_label: "已暂存 diff", aria_pressed: *change_staged.read(), onclick: move |_| open_change(staged_path.clone(), true), "已暂存" }
                                                        }
                                                        label { class: "review-mark-control", title: "本地阅读进度，刷新变更后重置。", input { r#type: "checkbox", aria_label: "已审阅（仅本地）", checked: reviewed_snapshot.contains(&reviewed_path), onchange: move |event| {
                                                            if event.checked() { reviewed_changes.write().insert(reviewed_path.clone()); } else { reviewed_changes.write().remove(&reviewed_path); }
                                                        } } "标记已审阅" }
                                                    }
                                                }
                                            }
                                            if let Some(patch) = change_diff.read().as_ref() {
                                                div { class: "selected-diff-scroll", {render_diff(&patch.patch)} }
                                            } else if let Some(error) = change_error.read().as_ref() {
                                                div { class: "change-preview-empty error-text", strong { "Git patch 读取失败" } p { "{error}" }
                                                    button { onclick: move |_| {
                                                        let staged = *change_staged.peek();
                                                        open_change(path.clone(), staged);
                                                    }, "重试读取 diff" }
                                                }
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
                                QueueSync { key: "queue-sync-{session_generation}", session_id, client: shared_client, session_generation, queue, revision: queue_revision }
                            }
                        }
                        if active_view.read().as_str() == "conversation" {
                            if let Some(session_id) = selected_session.read().clone() {
                                scroll_follow::ScrollFollow { scope: session_scope_key(&node_url.read(), &session_id), selector: ".transcript".to_owned() }
                            }
                            div { class: "transcript-frame",
                            div { id: "conversation-panel", role: "tabpanel", aria_labelledby: "conversation-tab", class: "transcript", "data-session-scope": selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                if let Some(session_id) = selected_session.read().clone() {
                                    history_pager::HistoryPager {
                                        key: "history-pager-{session_generation}",
                                        scope: session_scope_key(&node_url.read(), &session_id), generation: *session_generation.read(),
                                        current_generation: session_generation, client: shared_client, session_id,
                                        window: history_window, transcript,
                                        native_queries: session_supports("get_message_summaries") && session_supports("get_message_window"),
                                        timeline_visible: timeline_visible(),
                                    }
                                }
                                if pending_reply.is_none() && pending_dialogs.read().is_empty() && transcript.read().is_empty() && projection.read().is_none() {
                                    div { class: "empty-state workspace-welcome",
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
                                    for (key, message, block, user_key, entry, pending) in display_rows {
                                        div { key: "message-row-{key}", class: if pending { "pending-user-message" } else if message.is_some() { "message-turn" } else { "projection-message-row" }, "data-message-row": "{key}", "data-history-id": entry.clone(), "data-user-message-key": user_key,
                                            if let Some(message) = message.as_ref() {
                                                {
                                                    if message.get("role").or_else(|| message.get("kind")).and_then(Value::as_str) == Some("user") {
                                                        conversation::render_history(message)
                                                    } else {
                                                        conversation::render_history_in_context(message, &transcript.read(), projection.read().as_ref().map_or(&[], |run| run.blocks.as_slice()))
                                                    }
                                                }
                                            } else if let Some(block) = block.as_ref() {
                                                {conversation::render_message_block(block)}
                                            }
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
                            if timeline_visible() {
                            if let Some(session_id) = selected_session.read().as_ref() {
                                user_timeline::UserTimeline {
                                    key: "user-timeline-{session_generation}",
                                    scope: session_scope_key(&node_url.read(), session_id), transcript,
                                    history: history_window, client: shared_client, session_id: session_id.clone(),
                                    native_queries: session_supports("get_message_summaries") && session_supports("get_message_window"),
                                }
                            }
                            }
                            }
                        div { class: "composer-main",
                            if session_supports("get_queue") {
                                if let Some(id) = selected_session.read().as_ref() {
                                    { let scope = session_scope_key(&node_url.read(), id); rsx! {
                                        for key in [format!("{scope}::{session_generation}")] {
                                            message_queue::QueuePanel {
                                                key: "{key}", snapshot: queue.read().clone(),
                                                history: queue_history.read().get(&scope).cloned(),
                                                busy: queue_busy.read().contains(&scope),
                                                error: queue_errors.read().get(&scope).cloned(),
                                                can_cancel: transport_ready && session_supports("cancel_queued"),
                                                can_clear: transport_ready && session_supports("clear_queue"),
                                                can_send: queue_can_send(),
                                                onaction: move |action| queue_action(action),
                                            }
                                        }
                                    } }
                                }
                            }
                            div { class: if running { "composer is-running" } else { "composer" },
                            if let Some(session_id) = selected_session.read().clone() {
                                { let scope = session_scope_key(&node_url.read(), &session_id); rsx! { composer_extras::AttachmentPicker { scope: scope.clone(), store: attachment_store, presentation: "files".to_owned(), images_supported: session_access.read().as_ref().is_some_and(|access| access.supports_prompt_content()), disabled: submitting, onquote: move |text| {
                                    if let Some(id) = selected_session.read().as_deref() { let draft_scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&draft_scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &draft_scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); }
                                } } } }
                            }
                            composer_input::ComposerInput {
                                key: "composer-{node_url}-{selected_session.read().as_deref().unwrap_or_default()}",
                                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                drafts,
                                disabled: !*drafts_ready.read() || selected_session.read().is_none() || projection.read().is_none(),
                                placeholder: if *new_session_busy.read() && selected_session.read().is_none() { "正在准备新会话…" } else { "继续描述你的想法，或输入 / 使用命令…" },
                                onenter: move |()| {
                                    if composer_cancelling() {
                                        return;
                                    }
                                    // /new changes this signal; release the read guard before dispatch.
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
                            div { class: "composer-capabilities",
                                button { class: "button", aria_label: "BTW", onclick: move |_| { if selected_session.read().is_some() { btw_open.set(true); } else { status.set("请先打开主会话，再创建独立旁问。".into()); } }, Icon { name: "message" } "BTW 旁问" }
                                details { class: "capability-menu",
                                summary { Icon { name: "blocks" } "常用能力" }
                                div { class: "capability-menu-items",
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
                                        { let scope = session_scope_key(&node_url.read(), &session_id); rsx! { composer_extras::AttachmentPicker { scope, store: attachment_store, presentation: "button".to_owned(), images_supported: session_access.read().as_ref().is_some_and(|access| access.supports_prompt_content()), disabled: submitting, onquote: move |text| {
                                            if let Some(id) = selected_session.read().as_deref() { let scope = session_scope_key(&node_url.read(), id); let existing = drafts.read().get(&scope).map(|d| d.text.clone()).unwrap_or_default(); update_draft(drafts, &scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") }); }
                                        } } } }
                                    }
                                    if let Some(id) = selected_session.read().clone().filter(|_| composer_protocol.read().as_ref().is_some_and(|(scope, protocol)| scope == &model_context() && protocol == "acp")) {
                                        if execution_backend::COMPOSER_CONTROLS_VISIBLE {
                                            execution_backend::NegotiatedConfig { key: "acp-config-{id}-{session_generation}", shared_client, session_id: id, session_generation, requests, idle: !extension_locked, initial_configuration: composer_configuration.read().clone() }
                                        }
                                    } else {
                                    div { class: "composer-model-controls", key: "config-pickers-{session_generation}-{node_connection.epoch}",
                                    model_picker::ModelPicker {
                                        models: available_models.read().clone(), current_model: composer_model, current_provider: composer_provider,
                                        loading: *model_catalog_loading.read(), error: model_catalog_error.read().clone(),
                                        read_supported: *model_catalog_readable.read(), write_supported: *model_catalog_writable.read(),
                                        idle: !extension_locked, busy: model_switching, thinking_busy: thinking_switching, commit_revision: *model_commit_revision.read(),
                                        verification_required: config_verification_required,
                                        onrefresh: move |()| refresh_model_config(),
                                        onselect: move |model: ModelDescriptor| {
                                            let context = model_context();
                                            if config_needs_verification() || model_pending.read().as_ref() == Some(&context) || thinking_pending.read().as_ref() == Some(&context) || extension_locked || !*model_catalog_writable.read()
                                                || !session_supports("get_state") || !model_picker::selectable_model(&available_models.read(), &model) { return; }
                                            let (Some(session_id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) else { return; };
                                            model_pending.set(Some(context.clone())); model_catalog_error.set(None);
                                            state::scoped_spawn(requests, async move {
                                                let result = config_requests::bounded("切换模型", client.set_session_config_option(&session_id, "model", serde_json::json!(model.id))).await;
                                                if model_context() != context { return; }
                                                match result {
                                                    Ok(response) if response.get("requiresRestart").and_then(Value::as_bool) != Some(true) => {
                                                        config_unconfirmed.set(Some((context.clone(), ConfigField::Model)));
                                                        let state = config_requests::bounded("回读模型", client.runtime_state(&session_id)).await;
                                                        if model_context() != context { return; }
                                                        match state {
                                                            Ok(state) => {
                                                                let confirmed = model_picker::matches_model(&model, state.provider.as_deref().unwrap_or_default(), state.model_id.as_deref().unwrap_or_default());
                                                                if confirmed { runtime_state.set(Some(state)); config_unconfirmed.set(None); thinking_error.set(None); model_commit_revision += 1; }
                                                                else { model_catalog_error.set(Some("节点回读的模型与所选模型不一致，请刷新核对后重试".into())); }
                                                            }
                                                            Err(error) => {
                                                                if matches!(&error, work_client::ClientError::RequestTimeout(_)) {
                                                                    config_unconfirmed.set(Some((context.clone(), ConfigField::Model)));
                                                                    status.set("模型配置请求超时，结果待核对；请打开 Provider 刷新核对当前配置".into());
                                                                }
                                                                model_catalog_error.set(Some(format!("模型选择已提交，但回读状态失败：{error}；请刷新核对")));
                                                            }
                                                        }
                                                    }
                                                    Ok(_) => {
                                                        config_unconfirmed.set(Some((context.clone(), ConfigField::Model)));
                                                        model_catalog_error.set(Some("节点要求重启后生效，当前模型尚未确认；请在节点端完成重启后刷新".into()));
                                                    }
                                                    Err(error) => {
                                                        if matches!(&error, work_client::ClientError::RequestTimeout(_)) {
                                                            config_unconfirmed.set(Some((context.clone(), ConfigField::Model)));
                                                            status.set("模型配置请求超时，结果待核对；请打开 Provider 刷新核对当前配置".into());
                                                        }
                                                        model_catalog_error.set(Some(format!("切换模型失败：{error}")));
                                                    }
                                                }
                                                if model_pending.peek().as_ref() == Some(&context) { model_pending.set(None); }
                                            });
                                        },
                                    }
                                    model_picker::ThinkingPicker {
                                        levels: available_thinking_levels.read().clone(), current_level: composer_thinking,
                                        disabled: extension_locked || model_switching, write_supported: *thinking_writable.read(),
                                        busy: thinking_switching,
                                        loading: *thinking_loading.read(), load_error: thinking_load_error.read().clone(),
                                        verification_required: config_verification_required,
                                        onrefresh: move |()| refresh_model_config(),
                                        error: thinking_error.read().as_ref().filter(|(scope, _)| scope == &model_context()).map(|(_, message)| message.clone()),
                                        commit_revision: *thinking_commit_revision.read(),
                                        onselect: move |value: String| {
                                            let context = model_context();
                                            if config_needs_verification() || model_pending.read().as_ref() == Some(&context) || thinking_pending.read().as_ref() == Some(&context) || extension_locked || !*thinking_writable.read() || !session_supports("get_state") { return; }
                                            if value.trim().is_empty() || !available_thinking_levels.read().contains(&value) { return; }
                                            let (Some(session_id), Some(client)) = (selected_session.read().clone(), shared_client.read().clone()) else { return; };
                                            thinking_error.set(None);
                                            thinking_pending.set(Some(context.clone()));
                                            state::scoped_spawn(requests, async move {
                                                let result = config_requests::bounded("切换思考等级", client.set_session_config_option(&session_id, "thinking", serde_json::json!(value))).await;
                                                if model_context() != context { return; }
                                                if matches!(&result, Err(work_client::ClientError::RequestTimeout(_))) { config_unconfirmed.set(Some((context.clone(), ConfigField::Thinking))); }
                                                let failure = match result {
                                                    Ok(response) if response.get("requiresRestart").and_then(Value::as_bool) != Some(true) => {
                                                        config_unconfirmed.set(Some((context.clone(), ConfigField::Thinking)));
                                                        let result = config_requests::bounded("回读思考等级", client.runtime_state(&session_id)).await;
                                                        if model_context() != context { return; }
                                                        if matches!(&result, Err(work_client::ClientError::RequestTimeout(_))) { config_unconfirmed.set(Some((context.clone(), ConfigField::Thinking))); }
                                                        match result {
                                                            Ok(state) if state.thinking_level.as_deref() == Some(value.as_str()) => {
                                                                runtime_state.set(Some(state));
                                                                config_unconfirmed.set(None);
                                                                thinking_commit_revision += 1;
                                                                status.set("思考等级已更新".into());
                                                                None
                                                            }
                                                            Ok(_) => Some("节点回读的思考等级与所选等级不一致，请核对后重试".into()),
                                                            Err(error) => Some(format!("思考等级已提交，但回读失败：{error}")),
                                                        }
                                                    }
                                                    Ok(_) => {
                                                        config_unconfirmed.set(Some((context.clone(), ConfigField::Thinking)));
                                                        Some("节点要求重启后生效，当前思考等级尚未确认；请在节点端完成重启后刷新".into())
                                                    }
                                                    Err(error) => Some(format!("思考等级更新失败：{error}")),
                                                };
                                                if let Some(message) = failure {
                                                    status.set(message.clone());
                                                    thinking_error.set(Some((context.clone(), message)));
                                                }
                                                if thinking_pending.peek().as_ref() == Some(&context) { thinking_pending.set(None); }
                                            });
                                        },
                                    }
                                    }
                                    }
                                }
                                div { class: "composer-actions",
                                    button {
                                        class: if composer_stop { "send-button is-stop is-loading" } else if composer_loading { "send-button is-loading" } else if composer_continue.is_some() { "send-button is-continue" } else { "send-button" },
                                        disabled: (!composer_stop && (config_verification_required || model_switching || thinking_switching || submitting || composer_loading || continuation_disabled || (composer_empty && composer_continue.is_none()))) || !transport_ready || selected_session.read().is_none() || projection.read().is_none() || interaction == InteractionState::Cancelling || (if composer_stop { !session_supports("abort") } else if composer_continue == Some(ContinueAction::Resume) { !resume_supported() } else if interaction.can_queue() { !session_supports("queue_message") } else { !session_supports("prompt") }),
                                        aria_label: if composer_stop { if interaction == InteractionState::Cancelling { "正在停止…" } else { "停止运行" } } else if composer_loading { activity_label.unwrap_or("正在处理…") } else if composer_continue.is_some() { "继续会话" } else if interaction.can_queue() { "加入队列" } else { "发送消息" },
                                        aria_busy: composer_loading || composer_stop,
                                        title: if composer_stop { "停止当前任务" } else if composer_loading { "等待当前任务完成" } else if continuation_disabled { "当前节点不支持继续此会话" } else if composer_continue == Some(ContinueAction::Resume) { "恢复节点报告的待恢复会话" } else if composer_continue.is_some() { "发送“继续”，接着上一轮执行" } else if interaction.can_queue() { "加入后续消息队列" } else { "发送消息" },
                                        onclick: move |event| { if composer_stop { abort(event); } else { send_or_command(); } },
                                        if composer_stop && interaction != InteractionState::Cancelling {
                                            span { class: "composer-stop-icon", "aria-hidden": "true" }
                                        } else if composer_loading {
                                            activity::Spinner {}
                                        } else if composer_continue.is_some() {
                                            Icon { name: "play" }
                                        } else {
                                            Icon { name: "arrow" }
                                        }
                                    }
                                }
                            }
                            }
                            div { class: "composer-foot",
                                span { role: "status", span { class: "status-dot" } "{status}" }
                                if let Some(session_id) = selected_session.read().as_ref() {
                                    traces::TraceEntry { scope: session_scope_key(&node_url.read(), session_id), store: traces }
                                }
                                span { if composer_continue.is_some() { "Enter 继续 · 输入内容后发送新消息" } else if interaction.can_queue() { "Enter 加入队列 · Shift + Enter 换行" } else { "Enter 发送 · Shift + Enter 换行" } }
                            }
                            runtime_stats::RuntimeFooter {
                                scope: selected_session.read().as_ref().map(|id| session_scope_key(&node_url.read(), id)).unwrap_or_default(),
                                runtime_state, projection, transcript, online: transport_ready,
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
                                button { disabled: !session_supports("get_queue"), onclick: move |_| {
                                    active_page.set("workspace".into()); active_view.set("conversation".into());
                                    if queue.read().as_ref().is_none_or(|snapshot| !snapshot.items.iter().any(|item| message_queue::QueueStatus::from_wire(item.status.as_deref()).is_active())) { status.set("没有待处理消息；运行时发送新消息即可排队。".into()); }
                                    let _ = document::eval("requestAnimationFrame(() => { document.querySelector('.composer-queue-heading[aria-expanded=false]')?.click(); document.querySelector('.composer-main')?.scrollIntoView({block:'nearest'}); });");
                                }, Icon { name: "message".to_owned() } "队列 {queue.read().as_ref().map(|q| q.items.iter().filter(|item| message_queue::QueueStatus::from_wire(item.status.as_deref()).is_active()).count()).unwrap_or(0)}" }
                            }
                        }
                        div { class: "inspector-card inspector-environment", "data-node-kind": workspace_kind,
                            div { class: "inspector-section-label", strong { "执行环境" } Icon { name: workspace_icon } }
                            div { class: "environment-card",
                                div { class: "environment-heading",
                                    span { class: "environment-icon", Icon { name: workspace_icon } }
                                    div { strong { "{workspace_label}" } small { "{inspector_protocol}" } }
                                    i { class: if matches!(*node_connection.phase.read(), connections::Phase::Online) { "environment-online" } else { "environment-offline" } }
                                }
                                small { class: "environment-origin", title: "{workspace_address}", "{workspace_address}" }
                                code { title: "{inspector_project_path}", "{inspector_project_path}" }
                            }
                            div { class: "data-line", span { "模型提供方" } span { "{inspector_provider}" } }
                            if super::local_node::is_local(&node_url.read()) {
                                div { class: "data-line", span { "执行位置" } span { "这台电脑" } }
                            } else {
                                div { class: "data-line", span { "执行位置" } span { if workspace_kind == "remote" { "远程节点（含局域网）" } else { "尚未选择" } } }
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
                        if let Some(session_id) = selected_session.read().as_ref() {
                            turn_timing::TimingInspector {
                                key: "timing-inspector-{session_generation}",
                                scope: session_scope_key(&node_url.read(), session_id), store: traces,
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
                                button { class: "button", disabled: extension_locked || config_verification_required || model_switching || thinking_switching || !resume_supported(), onclick: move |_| resume_session(), "恢复会话" }
                            }
                        }
                        }
                        div { class: "inspector-footer", BrandMark { class: "tiny-logomark" } div { span { "你的工具。你的模型。" } span { "你的工作方式。" } } }
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
                    Modal { id: "session-details", title: "会话设置", class: "session-details-modal resource-modal", onclose: move |()| { session_details_open.set(false); delete_confirmation.set(false); },
                        div { class: "modal-body",
                            div { class: "resource-segments session-settings-tabs", role: "tablist", aria_label: "会话设置分类",
                                for (tab, label) in [("details", "会话详情"), ("design", "设计引用")] {
                                    button { id: "session-settings-{tab}-tab", r#type: "button", role: "tab",
                                        class: if session_settings_tab.read().as_str() == tab { "active" } else { "" },
                                        aria_selected: session_settings_tab.read().as_str() == tab, aria_controls: "session-settings-{tab}-panel",
                                        onclick: move |_| session_settings_tab.set(tab.into()), "{label}"
                                    }
                                }
                            }
                            if session_settings_tab.read().as_str() == "details" {
                            section { id: "session-settings-details-panel", role: "tabpanel", aria_labelledby: "session-settings-details-tab",
                            p { {selected_session_title.read().as_deref().filter(|s| !s.trim().is_empty()).unwrap_or(if selected_session.read().is_some() { "未命名会话" } else { "未选择会话" }).to_owned()} }
                            if let Some(runtime) = runtime_state.read().as_ref() {
                                dl { class: "runtime-list",
                                    div { dt { "Provider" } dd { "{runtime.provider.as_deref().unwrap_or(\"节点未返回\")}" } }
                                    div { dt { "模型" } dd { "{runtime.model_id.as_deref().unwrap_or(\"节点未返回\")}" } }
                                    div { dt { "工具" } dd { "{runtime.active_tools.join(\", \")}" } }
                                }
                            }
                            session_config::SessionConfig { shared_client, selected_session, session_generation, requests, runtime_state, status, tools_only: false, config_idle: !extension_locked, oncompact: move |()| { session_details_open.set(false); context_open.set(true); } }
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
                            }
                            } else {
                                section { id: "session-settings-design-panel", class: "session-resource-body", role: "tabpanel", aria_labelledby: "session-settings-design-tab",
                                    if let Some(session_id) = selected_session.read().clone() {
                                        for scope in [format!("resources-{}-{}-{}-{}", node_url, session_id, session_generation, node_connection.epoch)] {
                                        resources::SessionResources {
                                            key: "{scope}", embedded: true,
                                            session_id: session_scope_key(&node_url.read(), &session_id), compact: true, open_requested: open_session_config,
                                            native_project: sessions.read().iter().find(|s| s.session_id == session_id).and_then(|s| s.project_id.clone()),
                                            native_online: matches!(*node_connection.phase.read(), connections::Phase::Online),
                                            connection_epoch: node_connection.epoch,
                                            on_native_open: move |id: String| { session_details_open.set(false); pending_open.set(Some((false,id))); },
                                            backend_options: backend_options.read().clone(), selected_backend: selected_backend.read().clone(),
                                            backend_binding: session_binding.read().clone(), backend_busy: *new_session_busy.read(),
                                            onbackendchange: move |backend| { session_details_open.set(false); change_backend.call(backend); },
                                            session_title: selected_session_title.read().clone(),
                                            on_browse: Some(EventHandler::new(move |()| { session_details_open.set(false); active_page.set("packages".to_owned()); })),
                                            extension_config_locked: extension_locked, selected_session,
                                            library: resource_library, onquote: move |text: String| {
                                                if let Some(id) = selected_session.read().as_deref() {
                                                    let scope = session_scope_key(&node_url.read(), id);
                                                    let existing = drafts.read().get(&scope).map(|draft| draft.text.clone()).unwrap_or_default();
                                                    update_draft(drafts, &scope, if existing.is_empty() { text } else { format!("{existing}\n{text}") });
                                                }
                                            }, shared_client, session_generation, requests, runtime_state, status,
                                        }
                                        }
                                    } else { p { class: "meta", "选择会话后可管理设计引用与执行后端。" } }
                                }
                            }
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
                                        div { class: "project-preview-empty error-text", strong { "文件内容读取失败" } p { "{error}" } button { onclick: move |_| { let path = selected_project_file.peek().clone(); if let Some(path) = path { load_file(path, false); } }, "重试读取文件" } }
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
                                            if project_file_kind(&content.path) != "image" {
                                                button { class: if project_file_mode.read().as_str() == "source" { "active" } else { "" }, onclick: move |_| project_file_mode.set("source".into()), "源码" }
                                                button { onclick: move |_| { let wrap = *project_file_wrap.read(); project_file_wrap.set(!wrap); }, if *project_file_wrap.read() { "关闭换行" } else { "自动换行" } }
                                            }
                                            button { class: "project-quote", onclick: { let path = content.path.clone(); move |_| quote_project_file(path.clone()) }, "引用路径到草稿" }
                                        }
                                        div { class: "project-preview-content",
                                            if let Some(image) = file_image.read().as_ref() {
                                                message_image::MessageImage { image: image.clone(), project: true }
                                            } else {
                                                {render_project_file_at(content, project_file_mode.read().as_str(), *project_file_wrap.read(), *project_file_lines.read())}
                                            }
                                        }
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
                        current_session: selected_session.read().clone(), states: switcher_states,
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
