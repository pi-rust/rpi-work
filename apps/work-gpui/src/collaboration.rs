//! The connected node and credential-free nearby discovery.

use gpui::{ClipboardItem, Context, Div, FontWeight, Task, div, prelude::*, px};
use work_client::DiscoveredNode;
use work_viewmodel::connection::normalize_address;

use super::Shell;
use crate::icons::icon;

#[derive(Default)]
pub(super) struct Panel {
    discovery: Option<Task<()>>,
    nearby: Option<Result<Vec<DiscoveredNode>, ()>>,
    copied: Option<String>,
}

impl Shell {
    fn refresh_nearby_nodes(&mut self, cx: &mut Context<Self>) {
        let Some(node) = self.node.as_ref() else {
            return;
        };
        if self.collaboration.discovery.is_some() {
            return;
        }
        self.collaboration.copied = None;
        let read = node.discover_nearby();
        self.collaboration.discovery = Some(cx.spawn(async move |this, cx| {
            let result = read.await.ok().and_then(Result::ok).ok_or(());
            let _ = this.update(cx, |shell, cx| {
                shell.collaboration.discovery = None;
                shell.collaboration.nearby = Some(result);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn collaboration_settings(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        if self.node_connection.open {
            return self.node_connection_form(cx);
        }
        if self.node_share.open {
            return self.node_share_form(compact, cx);
        }
        let node = self.node.as_ref();
        let address = normalize_address(&self.snapshot.node, false).ok();
        let identity = self.snapshot.node_id.as_deref();
        let loading = self.collaboration.discovery.is_some();
        let nearby = match &self.collaboration.nearby {
            Some(Ok(nodes)) if !loading => {
                let mut rows = nodes
                    .iter()
                    .filter(|node| Some(node.probe.node_id()) != identity)
                    .filter_map(|node| {
                        let endpoint = normalize_address(&node.endpoint, false).ok()?;
                        (Some(&endpoint) != address.as_ref()).then(|| {
                            let detail = if node.probe.auth.required {
                                "需要登录"
                            } else {
                                "无需登录"
                            };
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .py_2()
                                .border_t_1()
                                .border_color(palette.line)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(self.node_address(&endpoint))
                                        .child(self.text(detail, 12.0, palette.muted)),
                                )
                                .child(self.connect_node_button(
                                    &endpoint,
                                    Some(node.probe.node_id().to_owned()),
                                    "connect-nearby",
                                    cx,
                                ))
                                .child(self.copy_node_address(&endpoint, cx))
                        })
                    })
                    .peekable();
                if rows.peek().is_some() {
                    div().children(rows)
                } else {
                    div().child(self.text("未发现其他节点。", 13.0, palette.muted))
                }
            }
            Some(Err(())) if !loading => {
                div().child(self.text("无法完成局域网发现。可以再次刷新。", 13.0, palette.danger))
            }
            _ if loading => div().child(self.text("正在查找附近节点…", 13.0, palette.muted)),
            _ => div().child(self.text("刷新以查找附近节点。", 13.0, palette.muted)),
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .when_some(node, |body, node| {
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_4()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(palette.line)
                        .bg(palette.subtle)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(
                                    if node.is_local() { "laptop" } else { "globe" },
                                    18.0,
                                    palette.muted,
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .child(
                                            self.text("当前节点", 14.0, palette.text)
                                                .font_weight(FontWeight::SEMIBOLD),
                                        )
                                        .child(self.text(
                                            if node.is_local() {
                                                "本机工作区"
                                            } else {
                                                "HTTP 节点"
                                            },
                                            12.0,
                                            palette.muted,
                                        )),
                                )
                                .child(self.text("已选用", 12.0, palette.muted)),
                        )
                        .when_some(address.as_ref(), |card, address| {
                            card.child(div().flex().child(self.node_address(address).flex_1()))
                        })
                        .when_some(
                            identity.filter(|id| !id.chars().any(char::is_control)),
                            |card, id| {
                                card.child(
                                    self.text(format!("节点 ID  {id}"), 12.0, palette.muted)
                                        .min_w_0(),
                                )
                            },
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .when_some(address.as_ref(), |row, address| {
                                    row.child(self.copy_node_address(address, cx))
                                })
                                .when(cfg!(target_os = "macos") && identity.is_some(), |row| {
                                    row.child(
                                        self.button("show-address-qr", "地址二维码")
                                            .border_1()
                                            .border_color(palette.line)
                                            .tab_index(0)
                                            .focus(|button| button.bg(palette.hover))
                                            .on_click(cx.listener(|shell, _, window, cx| {
                                                shell.show_node_share(window, cx)
                                            })),
                                    )
                                }),
                        ),
                )
            })
            .when(node.is_some_and(|node| !node.is_local()), |body| {
                body.child(self.text(
                    "请在节点所属电脑管理共享和签发一次性授权。",
                    13.0,
                    palette.muted,
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        self.text("连接其他节点", 14.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child({
                        let actions = div().flex().flex_wrap().justify_end().gap_2().child(
                            self.button("manual-node-address", "输入地址")
                                .border_1()
                                .border_color(palette.line)
                                .tab_index(0)
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.show_node_connection(None, window, cx)
                                })),
                        );
                        #[cfg(target_os = "macos")]
                        let actions = actions.child(
                            self.button("scan-node-address", "扫码连接")
                                .border_1()
                                .border_color(palette.line)
                                .tab_index(0)
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.choose_node_qr(window, cx)
                                })),
                        );
                        actions
                    }),
            )
            .when_some(self.node_connection_notice(), |body, notice| {
                body.child(self.text(notice.to_owned(), 13.0, palette.muted))
            })
            .when(node.is_some(), |body| {
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    self.text("附近节点", 14.0, palette.text)
                                        .font_weight(FontWeight::SEMIBOLD),
                                )
                                .child(
                                    self.button("refresh-nearby-nodes", "刷新发现")
                                        .border_1()
                                        .border_color(palette.line)
                                        .tab_index(0)
                                        .focus(|button| button.bg(palette.hover))
                                        .when(loading, |button| button.opacity(0.6))
                                        .on_click(cx.listener(|shell, _, _, cx| {
                                            shell.refresh_nearby_nodes(cx)
                                        })),
                                ),
                        )
                        .child(nearby),
                )
            })
            .when(
                self.node_connection
                    .history
                    .recent
                    .iter()
                    .any(|endpoint| Some(endpoint) != address.as_ref()),
                |body| {
                    body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                self.text("最近连接", 14.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .children(
                                self.node_connection
                                    .history
                                    .recent
                                    .iter()
                                    .filter(|endpoint| Some(*endpoint) != address.as_ref())
                                    .map(|endpoint| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .border_t_1()
                                            .border_color(palette.line)
                                            .py_1()
                                            .child(self.node_address(endpoint).flex_1())
                                            .child(self.connect_node_button(
                                                endpoint,
                                                None,
                                                "connect-recent",
                                                cx,
                                            ))
                                    }),
                            ),
                    )
                },
            )
    }

    fn connect_node_button(
        &self,
        address: &str,
        identity: Option<String>,
        kind: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let address = address.to_owned();
        self.button(
            gpui::SharedString::from(format!("{kind}-{address}")),
            "连接",
        )
        .border_1()
        .border_color(palette.line)
        .tab_index(0)
        .focus(|button| button.bg(palette.hover))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.show_node_connection(Some((address.clone(), identity.clone())), window, cx);
        }))
    }

    fn node_address(&self, address: &str) -> Div {
        self.text(address.to_owned(), 12.0, self.palette.text)
            .font_family("Menlo")
            .min_w_0()
            .truncate()
    }

    fn copy_node_address(&self, address: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let copied = self.collaboration.copied.as_deref() == Some(address);
        let address = address.to_owned();
        self.button(
            gpui::SharedString::from(format!("copy-node-address-{address}")),
            if copied { "已复制" } else { "复制地址" },
        )
        .border_1()
        .border_color(palette.line)
        .tab_index(0)
        .focus(|button| button.bg(palette.hover))
        .on_click(cx.listener(move |shell, _, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(address.clone()));
            shell.collaboration.copied = Some(address.clone());
            cx.notify();
        }))
    }
}
