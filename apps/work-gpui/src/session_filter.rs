//! Filter sidebar sessions by their node-reported workspace activity.

use std::{collections::HashMap, time::Duration};

use gpui::{Context, Div, FocusHandle, Stateful, Task, Window, div, prelude::*, px};
use tokio::task::AbortHandle;
use work_core::api::SessionActivity;

use super::Shell;
use crate::data::SessionItem;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Filter {
    #[default]
    All,
    Running,
    AwaitingApproval,
}

impl Filter {
    pub(super) fn matches(self, activity: Option<SessionActivity>) -> bool {
        match self {
            Self::All => true,
            Self::Running => matches!(
                activity,
                Some(SessionActivity::Running | SessionActivity::AwaitingApproval)
            ),
            Self::AwaitingApproval => activity == Some(SessionActivity::AwaitingApproval),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "全部会话",
            Self::Running => "执行中",
            Self::AwaitingApproval => "待审批",
        }
    }
}

#[derive(Default)]
enum Refresh {
    #[default]
    Ready,
    Loading,
    Failed,
}

pub(super) struct Panel {
    trigger: FocusHandle,
    filter: Filter,
    menu_open: bool,
    refresh: Refresh,
    poll: Option<Task<()>>,
}

impl Panel {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            trigger: cx.focus_handle(),
            filter: Filter::All,
            menu_open: false,
            refresh: Refresh::Ready,
            poll: None,
        }
    }

    pub(super) fn filter(&self) -> Filter {
        self.filter
    }
}

struct NodeScope {
    address: String,
    id: Option<String>,
}

impl NodeScope {
    fn matches(&self, shell: &Shell, filter: Filter) -> bool {
        shell.node.is_some()
            && shell.snapshot.node == self.address
            && shell.snapshot.node_id == self.id
            && shell.session_filter.filter == filter
    }
}

// Dropping a Tokio JoinHandle alone detaches the read.
struct AbortRead(AbortHandle);

impl Drop for AbortRead {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Shell {
    pub(super) fn session_filter_button(&self, cx: &Context<Self>) -> Stateful<Div> {
        let palette = self.palette;
        let active = self.session_filter.filter != Filter::All || self.session_filter.menu_open;
        self.navigation_tooltip(
            self.icon_button("filter-sessions", "list-checks", 25.0)
                .h(px(25.0))
                .when(active, |button| button.bg(palette.hover))
                .track_focus(&self.session_filter.trigger)
                .tab_index(0)
                .tab_stop(self.navigation_keyboard_enabled())
                .focus(|button| button.bg(palette.hover))
                .on_click(cx.listener(|shell, _, _, cx| {
                    shell.project_menu = None;
                    shell.session_filter.menu_open = !shell.session_filter.menu_open;
                    cx.notify();
                })),
            "筛选会话",
        )
    }

    pub(super) fn session_filter_controls(&self, cx: &Context<Self>) -> Option<Stateful<Div>> {
        let palette = self.palette;
        self.session_filter.menu_open.then(|| {
            div()
                .id("session-filter-menu")
                .absolute()
                .right_0()
                .top(px(38.0))
                .w(px(128.0))
                .occlude()
                .bg(palette.bg)
                .rounded(px(8.0))
                .border_1()
                .border_color(palette.line)
                .shadow_md()
                .p_1()
                .flex()
                .flex_col()
                .children(
                    [Filter::All, Filter::Running, Filter::AwaitingApproval]
                        .into_iter()
                        .map(|filter| {
                            let selected = filter == self.session_filter.filter;
                            self.button(filter.label(), "")
                                .w_full()
                                .justify_between()
                                .tab_index(0)
                                .tab_stop(self.navigation_keyboard_enabled())
                                .focus(|button| button.bg(palette.hover))
                                .when(selected, |button| button.bg(palette.subtle))
                                .child(self.text(filter.label(), 12.0, palette.text))
                                .when(selected, |button| {
                                    button.child(self.text("✓", 12.0, palette.muted))
                                })
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.session_filter.trigger.focus(window);
                                    shell.set_session_filter(filter, cx);
                                }))
                        }),
                )
        })
    }

    /// `has_matches` counts visible-status rows across both sidebar groups before collapse.
    pub(super) fn session_filter_notice(&self, has_matches: bool) -> Option<Div> {
        let filter = self.session_filter.filter;
        if filter == Filter::All {
            return None;
        }
        let palette = self.palette;
        let (message, color) = match self.session_filter.refresh {
            Refresh::Loading => ("正在读取会话状态…", palette.muted),
            Refresh::Failed => ("会话状态读取失败", palette.danger),
            Refresh::Ready => {
                let unknown = self.snapshot.sessions.iter().any(|session| {
                    !session.archived
                        && matches!(session.activity, None | Some(SessionActivity::Unknown))
                });
                let message = match (unknown, has_matches, filter) {
                    (true, _, _) => "有会话状态未知",
                    (false, false, Filter::Running) => "没有正在执行的会话",
                    (false, false, Filter::AwaitingApproval) => "没有等待审批的会话",
                    _ => return None,
                };
                (message, palette.muted)
            }
        };
        Some(self.text(message, 12.0, color).px_3().py_1())
    }

    pub(super) fn close_session_filter_menu(&mut self) -> bool {
        std::mem::take(&mut self.session_filter.menu_open)
    }

    pub(super) fn dismiss_session_filter_menu(&mut self, window: &mut Window) -> bool {
        if !self.close_session_filter_menu() {
            return false;
        }
        self.session_filter.trigger.focus(window);
        true
    }

    fn set_session_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.close_session_filter_menu();
        cx.notify();
        if self.session_filter.filter == filter {
            return;
        }
        self.session_filter.filter = filter;
        self.session_filter.poll = None;
        self.session_filter.refresh = Refresh::Ready;
        if filter == Filter::All || self.node.is_none() {
            return;
        }

        self.session_filter.refresh = Refresh::Loading;
        let scope = NodeScope {
            address: self.snapshot.node.clone(),
            id: self.snapshot.node_id.clone(),
        };
        let background = cx.background_executor().clone();
        self.session_filter.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some((revision, read, abort))) = this.update(cx, |shell, _| {
                    let node = shell
                        .node
                        .as_ref()
                        .filter(|_| scope.matches(shell, filter))?;
                    let read = node.sessions();
                    let abort = AbortRead(read.abort_handle());
                    Some((shell.session_catalog_revision, read, abort))
                }) else {
                    break;
                };
                let result = read.await;
                drop(abort);
                let current = this.update(cx, |shell, cx| {
                    if !scope.matches(shell, filter) {
                        return false;
                    }
                    if shell.session_catalog_revision == revision {
                        shell.session_filter.refresh = match result {
                            Ok(Ok(listed)) => {
                                // Creation owns the index assigned to its readback, including retries.
                                let append = shell.creating_session.is_none()
                                    && shell.creation_readback.is_none()
                                    && !shell.session_fork.in_flight();
                                let count = shell.snapshot.sessions.len();
                                merge_activity(&mut shell.snapshot.sessions, listed, append);
                                if shell.snapshot.sessions.len() != count {
                                    shell.session_catalog_revision =
                                        shell.session_catalog_revision.wrapping_add(1);
                                }
                                Refresh::Ready
                            }
                            _ => Refresh::Failed,
                        };
                        cx.notify();
                    }
                    true
                });
                if !matches!(current, Ok(true)) {
                    break;
                }
                background.timer(POLL_INTERVAL).await;
            }
        }));
    }
}

fn merge_activity(sessions: &mut Vec<SessionItem>, listed: Vec<SessionItem>, append: bool) {
    let mut indices = HashMap::with_capacity(sessions.len());
    for (index, session) in sessions.iter_mut().enumerate() {
        // Keep indices referenced by navigation history even if a listing omits a row.
        session.activity = None;
        indices.insert(session.id.clone(), index);
    }
    for session in listed {
        if let Some(&index) = indices.get(&session.id) {
            sessions[index].activity = if session.archived {
                Some(SessionActivity::Inactive)
            } else {
                session.activity
            };
        } else if append {
            indices.insert(session.id.clone(), sessions.len());
            sessions.push(session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Filter, SessionActivity};

    #[test]
    fn session_filter_matches_activity() {
        use SessionActivity::{AwaitingApproval, Inactive, Running, Unknown};

        for (activity, running, approval) in [
            (None, false, false),
            (Some(Unknown), false, false),
            (Some(Inactive), false, false),
            (Some(Running), true, false),
            (Some(AwaitingApproval), true, true),
        ] {
            assert!(Filter::All.matches(activity));
            assert_eq!(Filter::Running.matches(activity), running, "{activity:?}");
            assert_eq!(
                Filter::AwaitingApproval.matches(activity),
                approval,
                "{activity:?}"
            );
        }
    }
}
