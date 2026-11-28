//! The settings form opens a workbench with a fresh authenticated transport.

use gpui::{Context, Div, Task, Window, div, prelude::*, px};
use work_viewmodel::connection::{
    NodeHistory, NodePassword, native_path, normalize_address, persist_native,
};

use super::{Shell, collaboration::activate_open_node};
use crate::data::{Node, NodeAuthorization};

#[derive(Default)]
pub(super) struct Form {
    pub(super) open: bool,
    pub(super) password: bool,
    target: Option<(String, Option<String>)>,
    authorization: Option<NodeAuthorization>,
    pending: Option<Task<()>>,
    scanning: bool,
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

    pub(super) fn clear_connection_history(&mut self, cx: &mut Context<Self>) {
        let history = NodeHistory::default();
        self.node_connection.notice = Some(if persist_native(&history) {
            self.node_connection.history = history;
            "已清除最近连接。正在使用的窗口仍可继续工作。".into()
        } else {
            "无法清除最近连接。".into()
        });
        cx.notify();
    }

    pub(super) fn show_node_connection(
        &mut self,
        target: Option<(String, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_node_connection();
        self.show_settings_form();
        self.node_connection.open = true;
        self.search.clear();
        if let Some((address, _)) = &target {
            self.search.buffer.replace(None, address);
        }
        self.node_connection.target = target;
        self.search.focus.focus(window);
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    pub(super) fn choose_node_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.node_connection_busy() {
            return;
        }
        self.show_node_connection(None, window, cx);
        self.node_connection.scanning = true;
        let selection = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择节点二维码图片".into()),
        });
        let background = cx.background_executor().clone();
        self.node_connection.pending = Some(cx.spawn_in(window, async move |this, cx| {
            let result =
                match selection.await {
                    Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                        Some(path) => background
                            .spawn(async move { super::node_share::scan_node_code(path).map(Some) })
                            .await,
                        None => Ok(None),
                    },
                    Ok(Ok(None)) => Ok(None),
                    _ => Err("无法选择二维码图片。".into()),
                };
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |shell, cx| {
                    shell.node_connection.pending = None;
                    shell.node_connection.scanning = false;
                    match result {
                        Ok(Some(super::node_share::NodeCode::Address(address))) => {
                            shell.search.clear();
                            shell.search.buffer.replace(None, &address);
                        }
                        Ok(Some(super::node_share::NodeCode::Authorization(address, code))) => {
                            shell.node_connection.target = Some((address, None));
                            shell.node_connection.authorization = Some(code);
                            shell.focus_settings_title(window);
                            cx.notify();
                            return;
                        }
                        Ok(None) => {
                            shell.cancel_node_connection(window, cx);
                            return;
                        }
                        Err(error) => shell.node_connection.notice = Some(error),
                    }
                    shell.search.focus.focus(window);
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }

    pub(super) fn reset_node_connection(&mut self) {
        self.node_connection.open = false;
        self.node_connection.password = false;
        self.node_connection.target = None;
        self.node_connection.authorization = None;
        self.node_connection.pending = None;
        self.node_connection.scanning = false;
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
        let (address, expected) = if (self.node_connection.password
            || self.node_connection.authorization.is_some())
            && let Some(target) = self.node_connection.target.clone()
        {
            target
        } else {
            match normalize_address(&self.search.buffer.text, false) {
                Ok(address) => {
                    let expected = self
                        .node_connection
                        .target
                        .as_ref()
                        .filter(|(target, _)| target == &address)
                        .and_then(|(_, identity)| identity.clone());
                    (address, expected)
                }
                Err(error) => {
                    self.node_connection.notice = Some(error);
                    cx.notify();
                    return;
                }
            }
        };
        if self.is_current_node(&address, expected.as_deref()) {
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
        let authorization = self.node_connection.authorization.take();
        let using_authorization = authorization.is_some();
        let endpoint = address.clone();
        let pin = expected.clone();
        let connect = cx
            .background_executor()
            .spawn(async move { Node::open(endpoint, pin, credential, authorization) });
        self.node_connection.pending = Some(cx.spawn_in(window, async move |this, cx| {
            let result = connect.await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |shell, cx| {
                    shell.node_connection.pending = None;
                    match result {
                        Ok(Some((snapshot, node))) => {
                            let id = snapshot.node_id.clone();
                            if shell.is_current_node(&address, id.as_deref()) {
                                shell.cancel_node_connection(window, cx);
                                shell.node_connection.notice =
                                    Some("此节点已在当前窗口打开。".into());
                                return;
                            }
                            if activate_open_node(&address, id.as_deref(), window, cx) {
                                shell.cancel_node_connection(window, cx);
                                shell.node_connection.notice =
                                    Some("已切换到此节点的工作台窗口。".into());
                                return;
                            }
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
                            if using_authorization {
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
        let has_authorization = form.authorization.is_some();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.text(
                if has_authorization {
                    "确认一次性授权"
                } else if form.password {
                    "节点密码"
                } else {
                    "节点地址"
                },
                14.0,
                palette.text,
            ))
            .when(form.password || has_authorization, |body| {
                body.when_some(form.target.as_ref(), |body, (address, _)| {
                    body.child(self.text(address.clone(), 12.0, palette.muted).min_w_0())
                })
            })
            .when(!form.password && !has_authorization, |body| {
                body.child(self.text(
                    "输入对方提供的 RPI Work 节点地址。确认连接后会在独立窗口打开。",
                    13.0,
                    palette.muted,
                ))
            })
            .when(!has_authorization && !busy, |body| {
                body.child(
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
            })
            .when(busy, |body| {
                let body = if !form.scanning && !form.password && !has_authorization {
                    body.child(self.text(self.search.buffer.text.clone(), 12.0, palette.muted))
                } else {
                    body
                };
                let status = if form.scanning {
                    "正在识别二维码…"
                } else if form.password {
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
                    .flex_wrap()
                    .gap_2()
                    .when(form.password && !busy, |actions| {
                        actions.child(
                            self.dialog_button("edit-node-address", "更换地址", true, false)
                                .tab_index(0)
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    let target = shell.node_connection.target.clone();
                                    shell.show_node_connection(target, window, cx);
                                    shell.search.buffer.select_all();
                                })),
                        )
                    })
                    .child(
                        self.dialog_button("cancel-node-connection", "取消", true, false)
                            .tab_index(0)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.cancel_node_connection(window, cx)
                            })),
                    )
                    .when(!busy, |actions| {
                        actions.child(
                            self.dialog_button(
                                "submit-node-connection",
                                if has_authorization {
                                    "授权并打开"
                                } else if form.password {
                                    "登录并打开"
                                } else {
                                    "连接"
                                },
                                true,
                                true,
                            )
                            .tab_index(0)
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
