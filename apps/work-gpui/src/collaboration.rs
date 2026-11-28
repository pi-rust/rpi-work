//! The connected node and credential-free nearby discovery.

use gpui::{
    App, ClipboardItem, Context, Div, FontWeight, SharedString, Task, Window, WindowHandle, div,
    prelude::*, px,
};
use work_client::DiscoveredNode;
use work_viewmodel::connection::normalize_address;

use super::{Shell, feature_entries::Entry};
use crate::data::Node;
use crate::icons::icon;

#[derive(Default)]
pub(super) struct Panel {
    discovery: Option<Task<()>>,
    nearby: Option<Result<Vec<DiscoveredNode>, ()>>,
    copied: Option<String>,
    local: Option<work_host::http::Sharing>,
    local_pending: Option<Task<()>>,
    local_error: Option<String>,
}

struct OpenNode {
    window: WindowHandle<Shell>,
    local: bool,
    address: Option<String>,
    identity: Option<String>,
}

fn matches_node(
    address: &str,
    identity: Option<&str>,
    other_address: Option<&str>,
    other_identity: Option<&str>,
) -> bool {
    match (identity, other_identity) {
        (Some(identity), Some(other)) => identity == other,
        _ => other_address == Some(address),
    }
}

fn other_nodes(window: &Window, cx: &App) -> Vec<OpenNode> {
    let mut nodes = cx
        .windows()
        .into_iter()
        .filter(|handle| *handle != window.window_handle())
        .filter_map(|handle| {
            let window = handle.downcast::<Shell>()?;
            let shell = window.read(cx).ok()?;
            let local = shell.node.as_ref()?.is_local();
            Some(OpenNode {
                window,
                local,
                address: shell.collaboration_address(),
                identity: shell.collaboration_identity().map(str::to_owned),
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| (!a.local, &a.address).cmp(&(!b.local, &b.address)));
    nodes
}

pub(super) fn activate_open_node(
    address: &str,
    identity: Option<&str>,
    window: &Window,
    cx: &mut App,
) -> bool {
    other_nodes(window, cx)
        .into_iter()
        .filter(|node| {
            matches_node(
                address,
                identity,
                node.address.as_deref(),
                node.identity.as_deref(),
            )
        })
        .any(|node| {
            node.window
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        })
}

impl Shell {
    pub(super) fn is_current_node(&self, address: &str, identity: Option<&str>) -> bool {
        self.node.is_some()
            && matches_node(
                address,
                identity,
                self.collaboration_address().as_deref(),
                self.collaboration_identity(),
            )
    }

    pub(crate) fn collaboration_activated(&mut self, cx: &mut Context<Self>) {
        if self.settings.open {
            self.read_connection_history();
            cx.notify();
        }
    }

    pub(super) fn refresh_local_sharing(
        &mut self,
        change: Option<(&'static str, bool)>,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.node.as_ref().filter(|node| node.is_local()) else {
            return;
        };
        if self.collaboration.local_pending.is_some() {
            return;
        }
        self.collaboration.local_error = None;
        let result = node.local_sharing(change);
        self.collaboration.local_pending = Some(cx.spawn(async move |this, cx| {
            let result = result.await;
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.collaboration;
                panel.local_pending = None;
                match result {
                    Ok((sharing, error)) => {
                        panel.local = Some(sharing);
                        panel.local_error = error;
                    }
                    Err(error) => panel.local_error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn collaboration_address(&self) -> Option<String> {
        if self.node.as_ref()?.is_local() {
            let sharing = self
                .collaboration
                .local
                .as_ref()
                .filter(|sharing| sharing.enabled)?;
            normalize_address(&sharing.app_url, true).ok()
        } else {
            normalize_address(&self.snapshot.node, false).ok()
        }
    }

    pub(super) fn collaboration_identity(&self) -> Option<&str> {
        if self.node.as_ref()?.is_local() {
            let identity = &self.collaboration.local.as_ref()?.node_id;
            (!identity.is_empty() && !identity.chars().any(char::is_control))
                .then_some(identity.as_str())
        } else {
            self.snapshot
                .node_id
                .as_deref()
                .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))
        }
    }

    pub(super) fn stop_nearby_discovery(&mut self) {
        self.collaboration.discovery = None;
        self.collaboration.nearby = None;
    }

    pub(super) fn node_access_summary(&self) -> &'static str {
        match self.node.as_ref() {
            Some(node) if node.is_local() => "本机节点已就绪。",
            Some(_) => "当前窗口已连接远程节点。",
            None => "当前窗口尚未连接节点。",
        }
    }

    pub(super) fn web_access_summary(&self) -> &'static str {
        match self.node.as_ref() {
            None => "连接节点后查看 Web 访问状态。",
            Some(node) if !node.is_local() => "Web 访问由远程节点管理。",
            Some(_) if self.collaboration.local_pending.is_some() => "正在读取本机 Web 状态…",
            Some(_) => match &self.collaboration.local {
                Some(sharing) if sharing.web.listening && sharing.web.enabled => {
                    "本机 Web 访问已开启。"
                }
                Some(sharing) if sharing.web.listening => "本机 Web 入口可用，访问已关闭。",
                Some(sharing) if !sharing.web.status.is_empty() => "当前没有可用的本机 Web 入口。",
                _ => "本机 Web 访问状态未知。",
            },
        }
    }

    fn local_sharing_settings(&self, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let panel = &self.collaboration;
        let busy = panel.local_pending.is_some();
        let status = if busy {
            "正在读取本机访问状态…"
        } else {
            "尚未读取本机访问状态。"
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.collaboration_heading("其他设备访问本机"))
            .when_some(panel.local.as_ref(), |body, sharing| {
                body.child(self.local_access_row(
                    "允许远程协作",
                    "其他 RPI Work 客户端可以连接本机。启用后也会尝试在局域网发布本机。",
                    &sharing.status,
                    "node.sharing.write",
                    sharing.listening.then_some(sharing.enabled),
                    cx,
                ))
                .when(sharing.listening && sharing.enabled, |body| {
                    body.child(self.app_entry(cx))
                })
                .when(self.preferences.entries.shows(Entry::Web), |body| {
                    body.child(self.local_access_row(
                        "允许 Web 访问",
                        "通过独立的浏览器入口访问本机。",
                        &sharing.web.status,
                        "node.web.write",
                        sharing.web.listening.then_some(sharing.web.enabled),
                        cx,
                    ))
                })
                .when(!sharing.warning.is_empty(), |body| {
                    body.child(self.text(sharing.warning.clone(), 13.0, palette.danger))
                })
                .when(
                    self.preferences.entries.shows(Entry::Web) && sharing.web.enabled,
                    |body| {
                        body.when_some(
                            normalize_address(&sharing.web.url, false).ok(),
                            |body, address| {
                                body.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_2()
                                        .child(self.node_address(&address).w_full())
                                        .when(normalize_address(&address, true).is_err(), |row| {
                                            row.child(
                                                self.text(
                                                    "此网页地址仅供本机访问。",
                                                    12.0,
                                                    palette.muted,
                                                )
                                                .w_full(),
                                            )
                                        })
                                        .child(self.copy_node_address(
                                            "copy-web",
                                            "复制网页地址",
                                            &address,
                                            cx,
                                        ))
                                        .child(
                                            self.collaboration_button("open-web", "打开网页")
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    cx.open_url(&address)
                                                })),
                                        ),
                                )
                            },
                        )
                    },
                )
            })
            .when(panel.local.is_none(), |body| {
                body.child(self.text(status, 13.0, palette.muted))
            })
            .when_some(panel.local_error.as_ref(), |body, error| {
                body.child(self.text(error.clone(), 13.0, palette.danger))
            })
            .child(
                div().flex().child(
                    self.button("refresh-local-sharing", "刷新状态")
                        .tab_index(0)
                        .tab_stop(!busy)
                        .focus(|button| button.bg(palette.hover))
                        .when(!busy, |button| {
                            button.on_click(
                                cx.listener(|shell, _, _, cx| {
                                    shell.refresh_local_sharing(None, cx)
                                }),
                            )
                        }),
                ),
            )
    }

    fn local_access_row(
        &self,
        label: &str,
        description: &str,
        status: &str,
        capability: &'static str,
        enabled: Option<bool>,
        cx: &Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let busy = self.collaboration.local_pending.is_some();
        div()
            .flex()
            .items_center()
            .gap_2()
            .py_2()
            .border_t_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        self.text(label.to_owned(), 14.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(self.text(description.to_owned(), 12.0, palette.muted))
                    .child(self.text(
                        match enabled {
                            Some(true) => "已开启",
                            Some(false) => "已关闭",
                            None => "入口未就绪",
                        },
                        12.0,
                        palette.muted,
                    ))
                    .when(!status.is_empty(), |column| {
                        column.child(self.text(status.to_owned(), 12.0, palette.muted))
                    }),
            )
            .when_some(enabled, |row, enabled| {
                row.child(
                    self.button(capability, if enabled { "停用" } else { "开启" })
                        .border_1()
                        .border_color(palette.line)
                        .tab_index(0)
                        .tab_stop(!busy)
                        .focus(|button| button.bg(palette.hover))
                        .when(busy, |button| button.opacity(0.6))
                        .when(!busy, |button| {
                            button.on_click(cx.listener(move |shell, _, _, cx| {
                                shell.refresh_local_sharing(Some((capability, !enabled)), cx)
                            }))
                        }),
                )
            })
    }

    fn refresh_nearby_nodes(&mut self, cx: &mut Context<Self>) {
        if !self.preferences.entries.shows(Entry::Discovery)
            || self.collaboration.discovery.is_some()
        {
            return;
        }
        self.collaboration.copied = None;
        let read = cx
            .background_executor()
            .spawn(async { Node::discover_nearby() });
        self.collaboration.discovery = Some(cx.spawn(async move |this, cx| {
            let result = read.await.map_err(|_| ());
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
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        if self.node_connection.open {
            return self.node_connection_form(cx);
        }
        if self.node_share.open {
            return self.node_share_form(compact, cx);
        }
        let windows = other_nodes(window, cx);
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.current_node_card(windows, cx))
            .child(self.outgoing_connections(cx))
            .when(self.node.as_ref().is_some_and(Node::is_local), |body| {
                body.child(self.local_sharing_settings(cx))
            })
            .child(self.recent_connections(cx))
    }

    fn current_node_card(&self, windows: Vec<OpenNode>, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let local = self.node.as_ref().is_some_and(Node::is_local);
        let (title, detail, glyph) = match &self.node {
            Some(_) if local => ("这台电脑", "本机工作区 · 本机 RPC", "laptop"),
            Some(_) => ("远程节点", "当前窗口的项目和会话由此节点提供", "globe"),
            None => ("未连接节点", "通过节点地址或附近发现连接工作区", "globe"),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.line)
            .bg(palette.subtle)
            .child(self.collaboration_heading("当前窗口"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon(glyph, 18.0, palette.muted).flex_none())
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                self.text(title, 14.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(self.text(detail, 12.0, palette.muted)),
                    ),
            )
            .when(!local, |body| {
                body.when_some(self.collaboration_address(), |body, address| {
                    body.child(div().flex().child(self.node_address(&address).flex_1()))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(self.copy_node_address(
                                    "copy-current",
                                    "复制节点地址",
                                    &address,
                                    cx,
                                ))
                                .when(
                                    cfg!(target_os = "macos")
                                        && self.collaboration_identity().is_some(),
                                    |row| {
                                        row.child(
                                            self.collaboration_button(
                                                "show-remote-qr",
                                                "地址二维码",
                                            )
                                            .on_click(
                                                cx.listener(|shell, _, window, cx| {
                                                    shell.show_node_share(false, window, cx)
                                                }),
                                            ),
                                        )
                                    },
                                ),
                        )
                })
            })
            .when_some(self.collaboration_identity(), |body, identity| {
                body.child(
                    div().flex().child(
                        self.text(format!("节点 ID  {identity}"), 12.0, palette.muted)
                            .min_w_0()
                            .flex_1()
                            .truncate(),
                    ),
                )
            })
            .when(!windows.is_empty(), |body| {
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .mt_1()
                        .pt_2()
                        .border_t_1()
                        .border_color(palette.line)
                        .child(self.collaboration_heading("其他工作台窗口"))
                        .children(windows.into_iter().map(|node| {
                            let target = node.window;
                            let handle: gpui::AnyWindowHandle = target.into();
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .py_1()
                                .child(div().flex_1().min_w_0().child(if node.local {
                                    self.text("这台电脑 · 本机工作区", 13.0, palette.text)
                                } else {
                                    self.node_address(node.address.as_deref().unwrap_or("远程节点"))
                                }))
                                .child(
                                    self.collaboration_button(
                                        SharedString::from(format!(
                                            "switch-node-{:?}",
                                            handle.window_id()
                                        )),
                                        "切换窗口",
                                    )
                                    .on_click(cx.listener(
                                        move |_, _, window, cx| {
                                            window.defer(cx, move |_, cx| {
                                                let _ = target.update(cx, |_, window, _| {
                                                    window.activate_window()
                                                });
                                            });
                                        },
                                    )),
                                )
                        })),
                )
            })
    }

    fn outgoing_connections(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let loading = self.collaboration.discovery.is_some();
        let notice = self.node_connection_notice();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.collaboration_heading("连接其他节点"))
            .child(self.text(
                "连接成功后在独立窗口打开，可以回到原窗口继续工作。",
                12.0,
                palette.muted,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        self.collaboration_button("manual-node-address", "输入地址")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.show_node_connection(None, window, cx)
                            })),
                    )
                    .when(cfg!(target_os = "macos"), |row| {
                        #[cfg(target_os = "macos")]
                        let row = row.child(
                            self.collaboration_button("scan-node-address", "从二维码图片连接")
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.choose_node_qr(window, cx)
                                })),
                        );
                        row
                    }),
            )
            .when_some(notice, |body, notice| {
                body.child(self.text(notice.to_owned(), 13.0, palette.muted))
            })
            .when(self.preferences.entries.shows(Entry::Discovery), |body| {
                body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pt_2()
                        .child(self.collaboration_heading("附近节点"))
                        .child(
                            self.collaboration_button(
                                "refresh-nearby-nodes",
                                if loading {
                                    "正在发现…"
                                } else {
                                    "刷新发现"
                                },
                            )
                            .tab_stop(!loading)
                            .when(loading, |button| button.opacity(0.6))
                            .when(!loading, |button| {
                                button.on_click(
                                    cx.listener(|shell, _, _, cx| shell.refresh_nearby_nodes(cx)),
                                )
                            }),
                        ),
                )
                .child(self.nearby_nodes(cx))
            })
    }

    fn recent_connections(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt_2()
                    .child(self.collaboration_heading("最近连接"))
                    .when(!self.node_connection.history.recent.is_empty(), |row| {
                        row.child(
                            self.collaboration_button("clear-node-history", "清除记录")
                                .on_click(cx.listener(|shell, _, _, cx| {
                                    shell.clear_connection_history(cx)
                                })),
                        )
                    }),
            )
            .when(self.node_connection.history.recent.is_empty(), |body| {
                body.child(self.text("暂无远程连接记录。", 12.0, palette.muted))
            })
            .children(self.node_connection.history.recent.iter().map(|address| {
                let current = self.is_current_node(address, None);
                self.connection_row(
                    address,
                    None,
                    current.then_some("当前窗口"),
                    "connect-recent",
                    cx,
                )
            }))
    }

    fn nearby_nodes(&self, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let status = if self.collaboration.discovery.is_some() {
            "正在查找此设备同一局域网中的节点…"
        } else {
            match &self.collaboration.nearby {
                Some(Ok(nodes)) => {
                    let mut rows = nodes
                        .iter()
                        .filter_map(|node| {
                            let address = normalize_address(&node.endpoint, false).ok()?;
                            let identity = node.probe.node_id();
                            if self.is_current_node(&address, Some(identity)) {
                                return None;
                            }
                            let detail = if node.probe.auth.required {
                                "需要节点密码"
                            } else {
                                "无需登录"
                            };
                            Some(self.connection_row(
                                &address,
                                Some(identity),
                                Some(detail),
                                "connect-nearby",
                                cx,
                            ))
                        })
                        .peekable();
                    if rows.peek().is_some() {
                        return div().children(rows);
                    }
                    "未发现其他节点。可以直接输入地址。"
                }
                Some(Err(())) => "局域网发现失败。可以刷新重试或直接输入地址。",
                None => "刷新以查找同一局域网中发布的节点。",
            }
        };
        div().child(self.text(status, 12.0, palette.muted))
    }

    fn connection_row(
        &self,
        address: &str,
        identity: Option<&str>,
        detail: Option<&str>,
        kind: &str,
        cx: &Context<Self>,
    ) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .items_center()
            .gap_2()
            .py_1()
            .border_t_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.node_address(address))
                    .when_some(detail, |text, detail| {
                        text.child(self.text(detail.to_owned(), 12.0, palette.muted))
                    }),
            )
            .when(!self.is_current_node(address, identity), |row| {
                let target = (address.to_owned(), identity.map(str::to_owned));
                row.child(
                    self.collaboration_button(
                        SharedString::from(format!("{kind}-{address}")),
                        "连接",
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.show_node_connection(Some(target.clone()), window, cx)
                    })),
                )
            })
    }

    fn app_entry(&self, cx: &Context<Self>) -> Div {
        let address = self.collaboration_address();
        let can_share_qr = cfg!(target_os = "macos") && self.collaboration_identity().is_some();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(address.as_deref(), |body, address| {
                body.child(div().flex().child(self.node_address(address).flex_1()))
                    .child(div().flex().child(self.copy_node_address(
                        "copy-app",
                        "复制节点地址",
                        address,
                        cx,
                    )))
            })
            .when(address.is_none(), |body| {
                body.child(self.text(
                    if can_share_qr {
                        "暂无可分享的节点地址。如果已有对方可访问的地址，可以在生成二维码时手动填写。"
                    } else {
                        "暂无可分享的节点地址。请检查本机的远程协作入口配置。"
                    },
                    12.0,
                    self.palette.muted,
                ))
            })
            .when(can_share_qr, |body| {
                    body.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                self.collaboration_button("show-address-qr", "地址二维码")
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.show_node_share(false, window, cx)
                                    })),
                            )
                            .child(
                                self.collaboration_button(
                                    "issue-authorization-qr",
                                    "App 一次性授权",
                                )
                                .on_click(cx.listener(
                                    |shell, _, window, cx| shell.show_node_share(true, window, cx),
                                )),
                            ),
                    )
                },
            )
    }

    fn collaboration_heading(&self, title: &'static str) -> Div {
        self.text(title, 14.0, self.palette.text)
            .font_weight(FontWeight::SEMIBOLD)
    }

    fn collaboration_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &'static str,
    ) -> gpui::Stateful<Div> {
        self.button(id, label)
            .border_1()
            .border_color(self.palette.line)
            .tab_index(0)
            .focus(|button| button.border_color(self.palette.strong_line))
            .flex_none()
    }

    fn node_address(&self, address: &str) -> Div {
        self.text(address.to_owned(), 12.0, self.palette.text)
            .font_family("Menlo")
            .min_w_0()
            .truncate()
    }

    fn copy_node_address(
        &self,
        kind: &'static str,
        label: &'static str,
        address: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let copied = self.collaboration.copied.as_deref() == Some(kind);
        let address = address.to_owned();
        self.collaboration_button(kind, if copied { "已复制" } else { label })
            .on_click(cx.listener(move |shell, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(address.clone()));
                shell.collaboration.copied = Some(kind.into());
                cx.notify();
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::matches_node;

    #[test]
    fn verified_identity_joins_aliases_and_distinguishes_reassigned_addresses() {
        let address = "http://computer:13031";
        assert!(matches_node(
            address,
            Some("a"),
            Some("https://proxy"),
            Some("a")
        ));
        assert!(!matches_node(address, Some("a"), Some(address), Some("b")));
        assert!(matches_node(address, None, Some(address), Some("a")));
        assert!(!matches_node(
            address,
            None,
            Some("https://proxy"),
            Some("a")
        ));
    }
}
