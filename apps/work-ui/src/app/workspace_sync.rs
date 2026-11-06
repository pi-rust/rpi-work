//! Refresh node-owned catalogs without owning a session or an execution process.
use super::{connections, identity::project_scope_key};
use dioxus::{core::Task, prelude::*};
use std::collections::HashMap;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use work_client::{Project, SessionSummary, WorkspaceClient};

// Drive independent bounded requests without adding a platform-specific executor
// or letting one hung endpoint discard a successful response from the other.
async fn concurrently(left: impl Future<Output = ()>, right: impl Future<Output = ()>) {
    let mut left = pin!(left);
    let mut right = pin!(right);
    let (mut left_done, mut right_done) = (false, false);
    poll_fn(move |cx| {
        if !left_done {
            left_done = left.as_mut().poll(cx).is_ready();
        }
        if !right_done {
            right_done = right.as_mut().poll(cx).is_ready();
        }
        if left_done && right_done {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// Reject a response from an old connection or one predating a local mutation.
fn may_apply<T: PartialEq>(expected_epoch: u64, epoch: u64, before: &T, current: &T) -> bool {
    expected_epoch == epoch && before == current
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct RefreshState {
    pub epoch: u64,
    pub request: u64,
    pub busy: bool,
    pub message: String,
}

#[derive(Clone, Copy)]
enum RefreshResult {
    Applied,
    Changed,
    Failed,
}

fn refresh_message(sessions: RefreshResult, projects: RefreshResult) -> &'static str {
    match (sessions, projects) {
        (RefreshResult::Failed, _) => "刷新会话失败，已保留原列表；请检查连接或登录后重试。",
        (_, RefreshResult::Changed) | (RefreshResult::Changed, _) => {
            "刷新期间目录有变更，已保留本地更新；请再次刷新。"
        }
        (_, RefreshResult::Failed) => "会话已刷新，但项目刷新失败；请重试。",
        _ => "已拉取最新会话。",
    }
}

#[component]
pub(super) fn WorkspaceSync(
    address: String,
    epoch: u64,
    client: Signal<Option<WorkspaceClient>>,
    connection_epoch: Signal<u64>,
    mut sessions: Signal<Vec<SessionSummary>>,
    mut projects: Signal<Vec<Project>>,
    selected_session: Signal<Option<String>>,
    mut selected_session_title: Signal<Option<String>>,
    selected_project: Signal<Option<String>>,
    mut selected_project_name: Signal<Option<String>>,
    mut project_aliases: Signal<HashMap<String, String>>,
    mut refresh: Signal<RefreshState>,
) -> Element {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = use_hook(|| NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut task = use_signal(|| None::<Task>);
    let mut clock_ready = use_signal(|| false);
    use_effect(move || {
        let state = refresh.read().clone();
        if !*clock_ready.read() || state.request == 0 || state.epoch != epoch {
            return;
        }
        let _ = document::eval(&format!(
            "window.__rpiWorkspaceSync?.get({id})?.request({});",
            state.request
        ));
    });
    use_effect(move || {
        let Some(node) = client.read().clone() else {
            return;
        };
        if let Some(previous) = task.write().take() {
            previous.cancel();
        }
        let address = address.clone();
        if refresh.peek().epoch == epoch
            && !refresh.peek().busy
            && refresh.peek().message.is_empty()
        {
            let mut state = refresh.write();
            state.busy = true;
            state.message = "正在读取会话与项目…".into();
        }
        task.set(Some(spawn(async move {
            let script = include_str!("workspace_sync.js");
            let mut clock = document::eval(&format!("const id={id};{script}"));
            while let Ok(_manual) = clock.recv::<bool>().await {
                if *connection_epoch.peek() != epoch {
                    break;
                }
                if !*clock_ready.peek() {
                    clock_ready.set(true);
                }
                let mut session_result = RefreshResult::Failed;
                let mut project_result = RefreshResult::Failed;
                // Do not subscribe the effect to catalogs it updates itself.
                let before_sessions = sessions.peek().clone();
                let before_projects = projects.peek().clone();
                concurrently(
                    async {
                        let result = connections::with_timeout(node.sessions()).await;
                        if let Ok(Ok(items)) = result {
                            if may_apply(
                                epoch,
                                *connection_epoch.peek(),
                                &before_sessions,
                                &*sessions.peek(),
                            ) {
                                if let Some(selected) = selected_session.peek().as_ref()
                                    && let Some(session) =
                                        items.iter().find(|s| &s.session_id == selected)
                                    && *selected_session_title.peek() != session.title
                                {
                                    selected_session_title.set(session.title.clone());
                                }
                                session_result = RefreshResult::Applied;
                                if *sessions.peek() != items {
                                    sessions.set(items);
                                }
                            } else {
                                session_result = RefreshResult::Changed;
                            }
                        }
                    },
                    async {
                        let result = connections::with_timeout(node.projects()).await;
                        if let Ok(Ok(items)) = result {
                            if may_apply(
                                epoch,
                                *connection_epoch.peek(),
                                &before_projects,
                                &*projects.peek(),
                            ) {
                                // A node-confirmed rename supersedes a legacy display-name
                                // override, but unrelated per-device aliases remain intact.
                                for project in &items {
                                    if before_projects
                                        .iter()
                                        .any(|old| old.id == project.id && old.name != project.name)
                                    {
                                        project_aliases
                                            .write()
                                            .remove(&project_scope_key(&address, &project.id));
                                    }
                                }
                                if let Some(selected) = selected_project.peek().as_ref()
                                    && let Some(project) = items.iter().find(|p| &p.id == selected)
                                {
                                    let name = project_aliases
                                        .peek()
                                        .get(&project_scope_key(&address, selected))
                                        .cloned()
                                        .unwrap_or_else(|| project.name.clone());
                                    if selected_project_name.peek().as_ref() != Some(&name) {
                                        selected_project_name.set(Some(name));
                                    }
                                }
                                project_result = RefreshResult::Applied;
                                if *projects.peek() != items {
                                    projects.set(items);
                                }
                            } else {
                                project_result = RefreshResult::Changed;
                            }
                        }
                    },
                )
                .await;
                if *connection_epoch.peek() != epoch {
                    break;
                }
                // The first automatic snapshot is also surfaced as a bounded
                // catalog load: otherwise a desktop window can look empty
                // until the first periodic refresh completes. Manual refreshes
                // keep the same completion semantics.
                if refresh.peek().epoch == epoch && refresh.peek().busy {
                    let mut state = refresh.write();
                    state.busy = false;
                    state.message = refresh_message(session_result, project_result).into();
                }
                // Failures retain the last good catalogs and schedule a retry.
                let _ = document::eval(&format!(
                    "window.__rpiWorkspaceSync?.get({id})?.complete();"
                ));
            }
            let _ = document::eval(&format!("window.__rpiWorkspaceSync?.get({id})?.dispose();"));
        })));
    });
    use_drop(move || {
        let _ = document::eval(&format!("window.__rpiWorkspaceSync?.get({id})?.dispose();"));
        if let Some(task) = task.write().take() {
            task.cancel();
        }
    });
    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::{RefreshResult, may_apply, refresh_message};

    #[test]
    fn manual_refresh_does_not_claim_success_for_failed_or_superseded_lists() {
        assert_eq!(
            refresh_message(RefreshResult::Applied, RefreshResult::Applied),
            "已拉取最新会话。"
        );
        assert!(
            refresh_message(RefreshResult::Failed, RefreshResult::Applied).contains("刷新会话失败")
        );
        assert!(
            refresh_message(RefreshResult::Applied, RefreshResult::Failed).contains("项目刷新失败")
        );
        assert!(
            refresh_message(RefreshResult::Changed, RefreshResult::Applied).contains("请再次刷新")
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn a_waiting_endpoint_does_not_block_the_other_response() {
        let (sent, received) = tokio::sync::oneshot::channel();
        let (mut left_done, mut right_done) = (false, false);
        super::concurrently(
            async {
                received.await.unwrap();
                left_done = true;
            },
            async {
                right_done = true;
                sent.send(()).unwrap();
            },
        )
        .await;
        assert!(left_done && right_done);
    }

    #[test]
    fn refresh_requires_the_same_epoch_even_for_the_same_address_and_ids() {
        assert!(!may_apply(1, 2, &vec!["same-id"], &vec!["same-id"]));
        assert!(may_apply(2, 2, &vec!["same-id"], &vec!["same-id"]));
    }

    #[test]
    fn local_mutations_are_not_overwritten_by_a_response_already_in_flight() {
        assert!(!may_apply(1, 1, &vec!["old"], &vec!["new"]));
        assert!(!may_apply(1, 1, &vec!["archived"], &Vec::<&str>::new()));
        assert!(may_apply(1, 1, &Vec::<&str>::new(), &Vec::<&str>::new()));
    }
}
