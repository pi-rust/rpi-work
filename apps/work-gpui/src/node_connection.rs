//! The settings form opens a workbench with a fresh authenticated transport.

use gpui::{Context, Div, Task, Window, div, prelude::*, px};
use work_viewmodel::connection::{
    NodeHistory, NodePassword, native_path, normalize_address, persist_native,
};

use super::Shell;
use crate::data::Node;

#[derive(Default)]
pub(super) struct Form {
    pub(super) open: bool,
    pub(super) password: bool,
    target: Option<(String, Option<String>)>,
    pending: Option<Task<()>>,
    notice: Option<String>,
    pub(super) history: NodeHistory,
}

fn read_history() -> NodeHistory {
    native_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .as_ref()
        .map(NodeHistory::restore)
        .unwrap_or_default()
}

impl Shell {
    pub(super) fn read_connection_history(&mut self) {
        self.node_connection.history = read_history();
    }

    pub(super) fn show_node_connection(
        &mut self,
        target: Option<(String, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.node_connection.open = true;
        self.node_connection.notice = None;
        self.node_connection.target = target;
        self.search.clear();
        self.search.focus.focus(window);
        if self.node_connection.target.is_some() {
            self.submit_node_connection(window, cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn reset_node_connection(&mut self) {
        self.node_connection.open = false;
        self.node_connection.password = false;
        self.node_connection.target = None;
        self.node_connection.pending = None;
        self.node_connection.notice = None;
    }

    pub(super) fn cancel_node_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_node_connection();
        self.search.clear();
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(super) fn submit_node_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.node_connection.open
            || self.node_connection.pending.is_some()
            || !self.search.buffer.is_committed()
        {
            return;
        }
        let (address, expected) = if let Some(target) = self.node_connection.target.clone() {
            target
        } else {
            match normalize_address(&self.search.buffer.text, false) {
                Ok(address) => (address, None),
                Err(error) => {
                    self.node_connection.notice = Some(error);
                    cx.notify();
                    return;
                }
            }
        };
        if normalize_address(&self.snapshot.node, false).ok().as_ref() == Some(&address) {
            self.node_connection.notice = Some("此节点已在当前窗口打开。".into());
            cx.notify();
            return;
        }
        let credential = if self.node_connection.password {
            let mut password =
                NodePassword::new(std::mem::take(&mut self.search.buffer.text), &address);
            let Some(value) = password.take_for(&address) else {
                self.node_connection.notice = Some("请输入此节点的密码。".into());
                cx.notify();
                return;
            };
            self.search.clear();
            Some(value)
        } else {
            None
        };
        window.prevent_default();
        self.node_connection.notice = None;
        let authenticating = credential.is_some();
        let endpoint = address.clone();
        let pin = expected.clone();
        let connect = cx
            .background_executor()
            .spawn(async move { Node::open(endpoint, pin, credential) });
        self.node_connection.pending = Some(cx.spawn_in(window, async move |this, cx| {
            let result = connect.await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |shell, cx| {
                    shell.node_connection.pending = None;
                    match result {
                        Ok(Some((snapshot, node))) => {
                            let id = snapshot.node_id.clone();
                            if crate::open_workbench(snapshot, Some(node), cx).is_ok() {
                                let mut history = read_history();
                                history.remember_with_identity(&address, id.as_deref());
                                let saved = persist_native(&history);
                                shell.node_connection.history = history;
                                shell.cancel_node_connection(window, cx);
                                shell.node_connection.notice = Some(if saved {
                                    "已在新窗口打开节点。".into()
                                } else {
                                    "已打开节点，最近连接保存失败。".into()
                                });
                            } else {
                                shell.node_connection.notice =
                                    Some("无法打开新工作台窗口。".into());
                            }
                        }
                        Ok(None) => {
                            shell.node_connection.target = Some((address, expected));
                            shell.node_connection.password = true;
                            shell.search.clear();
                            shell.search.focus.focus(window);
                        }
                        Err(error) => {
                            shell.node_connection.notice = Some(if authenticating {
                                "登录或加载项目失败，请检查密码与节点状态。".into()
                            } else {
                                error
                            });
                            // A failed recent or discovered target returns to address entry.
                            if !authenticating {
                                shell.node_connection.target = None;
                                shell.search.clear();
                                shell.search.buffer.replace(None, &address);
                            }
                            shell.search.focus.focus(window);
                        }
                    }
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }

    pub(super) fn node_connection_form(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let form = &self.node_connection;
        let busy = form.pending.is_some();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.text(
                if form.password {
                    "节点密码"
                } else {
                    "节点地址"
                },
                14.0,
                palette.text,
            ))
            .when_some(form.target.as_ref(), |body, (address, _)| {
                body.child(self.text(address.clone(), 12.0, palette.muted).min_w_0())
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(38.0))
                    .px_2()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.strong_line)
                    .child(self.search_input_element(cx)),
            )
            .when(busy, |body| {
                let status = if form.password {
                    "正在登录并加载项目…"
                } else {
                    "正在连接…"
                };
                body.child(self.text(status, 13.0, palette.muted))
            })
            .when_some(form.notice.as_ref(), |body, notice| {
                body.child(self.text(notice.clone(), 13.0, palette.danger))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.button("cancel-node-connection", "取消")
                            .tab_index(0)
                            .focus(|button| button.bg(palette.hover))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.cancel_node_connection(window, cx)
                            })),
                    )
                    .when(!busy, |actions| {
                        actions.child(
                            self.button(
                                "submit-node-connection",
                                if form.password {
                                    "登录并打开"
                                } else {
                                    "连接"
                                },
                            )
                            .border_1()
                            .border_color(palette.line)
                            .tab_index(0)
                            .focus(|button| button.bg(palette.hover))
                            .on_click(cx.listener(
                                |shell, _, window, cx| shell.submit_node_connection(window, cx),
                            )),
                        )
                    }),
            )
    }

    pub(super) fn node_connection_busy(&self) -> bool {
        self.node_connection.pending.is_some()
    }

    pub(super) fn node_connection_notice(&self) -> Option<&str> {
        self.node_connection.notice.as_deref()
    }
}
