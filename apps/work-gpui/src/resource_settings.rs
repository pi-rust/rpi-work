//! Browse and inspect the selected project's installed resources.

use gpui::{Context, Div, FontWeight, ScrollHandle, Task, Window, div, point, prelude::*, px};
use work_core::api::{NativeCatalog, NativeKind, NativeResource};
use work_viewmodel::resources::catalog::{display_name, installation_origin, installed_resources};

use super::Shell;
use crate::icons::icon;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceTab {
    Skills,
    Plugins,
    Mcp,
}

impl ResourceTab {
    fn label(self) -> &'static str {
        match self {
            Self::Skills => "技能",
            Self::Plugins => "插件",
            Self::Mcp => "MCP 服务",
        }
    }

    fn kind(self) -> Option<NativeKind> {
        match self {
            Self::Skills => Some(NativeKind::Skill),
            Self::Plugins => Some(NativeKind::Extension),
            Self::Mcp => None,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Skills => "list-checks",
            Self::Plugins => "blocks",
            Self::Mcp => "globe",
        }
    }
}

#[derive(Default)]
pub(super) struct Panel {
    pending: Option<Task<()>>,
    catalog: Option<Result<NativeCatalog, ()>>,
    detail: Option<Detail>,
    scroll: ScrollHandle,
}

struct Detail {
    reference: String,
    inspection: Inspection,
}

enum Inspection {
    Ready,
    Pending { _task: Task<()> },
    Finished(bool),
}

impl Shell {
    pub(super) fn resource_page_open(&self) -> bool {
        self.resource_page.is_some()
    }

    pub(super) fn open_resource_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.navigation_keyboard_enabled() || self.composer_is_marked() {
            return;
        }
        self.close_session_filter_menu();
        self.project_menu = None;
        self.welcome_project_picker = false;
        self.overlay = None;
        self.input_dragging = false;
        self.browser.restore_composer_focus();
        if self.resource_page.is_none() {
            self.search.clear();
            self.navigation_revision += 1;
            self.resource_page = Some(ResourceTab::Skills);
            self.read_resource_catalog(cx);
        }
        self.search.focus.focus(window);
        cx.notify();
    }

    pub(super) fn close_resource_page(&mut self) -> bool {
        if self.resource_page.take().is_none() {
            return false;
        }
        self.reset_resource_catalog();
        self.search.clear();
        self.input_dragging = false;
        true
    }

    pub(super) fn return_from_resources(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let closed = self.close_resource_page();
        if closed {
            self.focus_composer(window);
            cx.notify();
        }
        closed
    }

    pub(super) fn reveal_resource_focus(&self, window: &Window) {
        if !self.resource_page_open() {
            return;
        }
        let scroll = &self.resource_settings.scroll;
        if self.resource_inspect_focus.is_focused(window) {
            scroll.scroll_to_bottom();
        } else if self.search.focus.is_focused(window) {
            scroll.set_offset(point(px(0.0), px(0.0)));
        }
    }

    pub(super) fn read_resource_catalog(&mut self, cx: &mut Context<Self>) {
        if self.resource_settings.pending.is_some() {
            return;
        }
        let (Some(node), Some(project)) = (&self.node, self.project) else {
            return;
        };
        let read = node.read_native_catalog(self.snapshot.projects[project].id.clone());
        self.resource_settings.catalog = None;
        self.resource_settings.detail = None;
        self.resource_settings.pending = Some(cx.spawn(async move |this, cx| {
            let result = read.await.ok().and_then(Result::ok).ok_or(());
            let _ = this.update(cx, |shell, cx| {
                shell.resource_settings.pending = None;
                shell.resource_settings.catalog = Some(result);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn reset_resource_catalog(&mut self) {
        self.resource_settings = Panel::default();
    }

    pub(super) fn clear_resource_detail(&mut self) {
        self.resource_settings.detail = None;
    }

    fn select_resource(&mut self, reference: String) {
        let selection = &mut self.resource_settings.detail;
        if selection
            .as_ref()
            .is_none_or(|detail| detail.reference != reference)
        {
            *selection = Some(Detail {
                reference,
                inspection: Inspection::Ready,
            });
        }
    }

    fn inspect_resource(&mut self, reference: String, cx: &mut Context<Self>) {
        self.select_resource(reference);
        let (Some(node), Some(project), Some(detail)) =
            (&self.node, self.project, &mut self.resource_settings.detail)
        else {
            return;
        };
        if matches!(detail.inspection, Inspection::Pending { .. }) {
            return;
        }
        let read = node.inspect_native_resource(
            self.snapshot.projects[project].id.clone(),
            detail.reference.clone(),
        );
        detail.inspection = Inspection::Pending {
            _task: cx.spawn(async move |this, cx| {
                let checked = matches!(read.await, Ok(Ok(receipt)) if receipt.success);
                let _ = this.update(cx, |shell, cx| {
                    if let Some(detail) = &mut shell.resource_settings.detail {
                        detail.inspection = Inspection::Finished(checked);
                        cx.notify();
                    }
                });
            }),
        };
        cx.notify();
    }

    pub(super) fn resource_page(
        &self,
        tab: ResourceTab,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let palette = self.palette;
        let compact = window.viewport_size().width <= px(700.0);
        let stacked = compact || width < 650.0;
        let project = self
            .project
            .and_then(|index| self.snapshot.projects.get(index));
        div()
            .id("resource-page")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.resource_settings.scroll)
            .px(px(if compact {
                17.0
            } else {
                (width * 0.05).clamp(20.0, 64.0)
            }))
            .py(px(if compact { 24.0 } else { 35.0 }))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .mb_5()
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                self.text(
                                    "技能与插件",
                                    if compact { 22.0 } else { 26.0 },
                                    palette.text,
                                )
                                .font_weight(FontWeight::SEMIBOLD),
                            )
                            .children(project.map(|project| {
                                self.text(project.name.clone(), 13.0, palette.muted).mt_1()
                            })),
                    )
                    .when(
                        project.is_some() && self.node.is_some() && tab.kind().is_some(),
                        |heading| heading.child(self.resource_refresh(cx)),
                    ),
            )
            .child(self.resource_toolbar(tab, stacked, window, cx))
            .child(if let Some(kind) = tab.kind() {
                self.resource_page_content(tab, kind, width, stacked, cx)
            } else {
                div().child(self.text("当前节点未提供 MCP 服务目录。", 14.0, palette.muted))
            })
    }

    fn resource_toolbar(
        &self,
        tab: ResourceTab,
        stacked: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let tabs = [ResourceTab::Skills, ResourceTab::Plugins, ResourceTab::Mcp];
        div()
            .flex()
            .gap_3()
            .mb_5()
            .when(stacked, |toolbar| toolbar.flex_col().items_start())
            .when(!stacked, |toolbar| toolbar.items_center().justify_between())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap_1()
                    .p_1()
                    .rounded(px(8.0))
                    .bg(palette.subtle)
                    .children(tabs.map(|entry| {
                        self.button(("resource-tab", entry as usize), "")
                            .px_3()
                            .tab_index(0)
                            .border_1()
                            .border_color(gpui::transparent_black())
                            .when(tab == entry, |button| button.bg(palette.bg).shadow_sm())
                            .focus(|button| button.border_color(palette.strong_line))
                            .child(self.text(
                                entry.label(),
                                13.0,
                                if tab == entry {
                                    palette.text
                                } else {
                                    palette.muted
                                },
                            ))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                if shell.resource_page != Some(entry) {
                                    shell.resource_page = Some(entry);
                                    shell.clear_resource_detail();
                                    cx.notify();
                                }
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .h(px(34.0))
                    .px_2()
                    .when(stacked, |field| field.w_full())
                    .when(!stacked, |field| field.w(px(190.0)))
                    .border_1()
                    .border_color(if self.search.focus.is_focused(window) {
                        palette.strong_line
                    } else {
                        palette.line
                    })
                    .rounded(px(7.0))
                    .child(icon("search", 14.0, palette.muted))
                    .child(self.search_input_element(cx)),
            )
    }

    fn resource_page_content(
        &self,
        tab: ResourceTab,
        kind: NativeKind,
        width: f32,
        stacked: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let message = match &self.resource_settings.catalog {
            _ if self.project.is_none() => "请先在工作台选择项目。",
            _ if self.node.is_none() => "连接节点后可查看资源。",
            None => "正在读取节点资源目录…",
            Some(Err(())) => {
                return div().child(self.text(
                    "当前节点无法提供资源目录。可刷新重试。",
                    14.0,
                    self.palette.danger,
                ));
            }
            Some(Ok(catalog)) => {
                return self.resource_page_catalog(tab, kind, catalog, width, stacked, cx);
            }
        };
        div().child(self.text(message, 14.0, self.palette.muted))
    }

    fn resource_page_catalog(
        &self,
        tab: ResourceTab,
        kind: NativeKind,
        catalog: &NativeCatalog,
        width: f32,
        stacked: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let query = self.search.buffer.text.trim().to_lowercase();
        let resources = installed_resources(catalog, false);
        let visible = resources
            .iter()
            .filter(|resource| resource.kind == kind && resource_matches(resource, &query));
        let detail = self.resource_settings.detail.as_ref();
        let selected = detail
            .and_then(|detail| {
                visible
                    .clone()
                    .find(|resource| resource.reference == detail.reference)
            })
            .or_else(|| visible.clone().next());
        let Some(selected) = selected else {
            let message = if query.is_empty() {
                "节点未报告此类型的已安装资源。"
            } else {
                "没有匹配的资源。"
            };
            return div().child(self.text(message, 14.0, palette.muted));
        };
        let inspection = detail
            .filter(|detail| detail.reference == selected.reference)
            .map_or(&Inspection::Ready, |detail| &detail.inspection);
        let narrow = width < 950.0;
        div()
            .flex()
            .items_start()
            .when(stacked, |layout| layout.flex_col().gap_5())
            .when(!stacked, |layout| {
                layout.gap(px(if narrow { 18.0 } else { 35.0 }))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .when(stacked, |list| list.w_full())
                    .when(!stacked, |list| list.flex_1())
                    .children(visible.map(|resource| {
                        self.resource_page_row(
                            tab,
                            resource,
                            catalog,
                            resource.reference == selected.reference,
                            cx,
                        )
                    })),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_none()
                    .border_color(palette.line)
                    .when(stacked, |detail| detail.border_t_1().w_full().pt(px(18.0)))
                    .when(!stacked, |detail| {
                        detail
                            .border_l_1()
                            .w(px(if narrow { 220.0 } else { 270.0 }))
                            .pl(px(if narrow { 18.0 } else { 25.0 }))
                    })
                    .child(self.resource_mark(tab, 48.0))
                    .child(
                        self.text(display_name(&selected.name), 18.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .mt_2(),
                    )
                    .child(
                        self.text(resource_caption(selected, catalog), 13.0, palette.muted)
                            .mt_1(),
                    )
                    .child(self.resource_detail(selected, inspection, cx)),
            )
    }

    fn resource_page_row(
        &self,
        tab: ResourceTab,
        resource: &NativeResource,
        catalog: &NativeCatalog,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        let reference = resource.reference.clone();
        self.button(gpui::SharedString::from(reference.clone()), "")
            .h_auto()
            .w_full()
            .min_h(px(68.0))
            .py_3()
            .px_3()
            .rounded(px(9.0))
            .gap_3()
            .justify_start()
            .tab_index(0)
            .border_1()
            .border_color(gpui::transparent_black())
            .when(selected, |row| {
                row.bg(palette.subtle).border_color(palette.line)
            })
            .focus(|row| row.border_color(palette.strong_line))
            .child(self.resource_mark(tab, 37.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        self.text(display_name(&resource.name), 15.0, palette.text)
                            .font_weight(FontWeight::MEDIUM)
                            .truncate(),
                    )
                    .child(
                        self.text(resource_caption(resource, catalog), 13.0, palette.muted)
                            .truncate(),
                    ),
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                shell.select_resource(reference.clone());
                cx.notify();
            }))
    }

    fn resource_mark(&self, tab: ResourceTab, size: f32) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .flex_none()
            .size(px(size))
            .items_center()
            .justify_center()
            .rounded(px(9.0))
            .border_1()
            .border_color(palette.strong_line)
            .bg(palette.bg)
            .child(icon(tab.glyph(), size * 0.46, palette.muted))
    }

    fn resource_refresh(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button("refresh-native-catalog", "刷新")
            .border_1()
            .border_color(palette.line)
            .tab_index(0)
            .focus(|button| button.bg(palette.hover))
            .when(self.resource_settings.pending.is_some(), |button| {
                button.opacity(0.6)
            })
            .on_click(cx.listener(|shell, _, _, cx| shell.read_resource_catalog(cx)))
    }

    pub(super) fn resource_settings(&self, kinds: &[NativeKind], cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let Some(project) = self
            .project
            .and_then(|index| self.snapshot.projects.get(index))
        else {
            return div().child(self.text("请先在工作台选择项目。", 13.0, palette.muted));
        };
        if self.node.is_none() {
            return div().child(self.text("连接节点后可查看资源。", 13.0, palette.muted));
        }
        let pending = self.resource_settings.pending.is_some();
        let body = div().flex().flex_col().gap_3().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    self.text(project.name.clone(), 14.0, palette.text)
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(self.resource_refresh(cx)),
        );
        match &self.resource_settings.catalog {
            None if pending => body.child(self.text("正在读取节点资源目录…", 13.0, palette.muted)),
            None => body,
            Some(Err(())) => body.child(self.text(
                "当前节点无法提供资源目录。可刷新重试。",
                13.0,
                palette.danger,
            )),
            Some(Ok(catalog)) => {
                let resources = installed_resources(catalog, false);
                body.children(kinds.iter().map(|&kind| {
                    let mut rows = resources
                        .iter()
                        .filter(|resource| resource.kind == kind)
                        .peekable();
                    let (label, empty) = match kind {
                        NativeKind::Extension => ("插件", "节点未报告已安装的插件。"),
                        NativeKind::Skill => ("技能", "节点未报告已安装的技能。"),
                        NativeKind::Package => ("包", "节点未报告已安装的包。"),
                    };
                    div()
                        .when(kinds.len() > 1, |group| {
                            group.child(
                                self.text(label, 15.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .mb_2(),
                            )
                        })
                        .when(rows.peek().is_none(), |group| {
                            group.child(self.text(empty, 13.0, palette.muted))
                        })
                        .children(rows.map(|resource| self.resource_row(resource, catalog, cx)))
                }))
            }
        }
    }

    fn resource_row(
        &self,
        resource: &NativeResource,
        catalog: &NativeCatalog,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        let caption = resource_caption(resource, catalog);
        let reference = resource.reference.clone();
        let expanded = self
            .resource_settings
            .detail
            .as_ref()
            .filter(|detail| detail.reference == reference);
        div()
            .id(gpui::SharedString::from(reference.clone()))
            .border_t_1()
            .border_color(palette.line)
            .py_2()
            .child(
                self.button("resource-detail", "")
                    .w_full()
                    .h_auto()
                    .min_h(px(44.0))
                    .px_0()
                    .justify_between()
                    .tab_index(0)
                    .focus(|button| button.bg(palette.subtle))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.text(display_name(&resource.name), 14.0, palette.text))
                            .child(self.text(caption, 12.0, palette.muted)),
                    )
                    .child(crate::icons::icon(
                        if expanded.is_some() {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        14.0,
                        palette.muted,
                    ))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        let selection = &mut shell.resource_settings.detail;
                        *selection = (!selection
                            .as_ref()
                            .is_some_and(|item| item.reference == reference))
                        .then(|| Detail {
                            reference: reference.clone(),
                            inspection: Inspection::Ready,
                        });
                        cx.notify();
                    })),
            )
            .when_some(expanded, |row, detail| {
                row.child(self.resource_detail(resource, &detail.inspection, cx))
            })
    }

    fn resource_detail(
        &self,
        resource: &NativeResource,
        inspection: &Inspection,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let pending = matches!(inspection, Inspection::Pending { .. });
        let reference = resource.reference.clone();
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .p_3()
            .mt_2()
            .rounded(px(8.0))
            .bg(palette.subtle)
            .when_some(
                resource.facts["source"]
                    .as_str()
                    .filter(|source| !source.trim().is_empty()),
                |body, source| {
                    body.child(self.text(format!("节点来源 · {source}"), 12.0, palette.muted))
                },
            )
            .when_some(
                resource.facts["description"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty()),
                |body, description| {
                    body.child(self.text(description.to_owned(), 13.0, palette.text))
                },
            )
            .child(
                self.button("inspect-native-resource", "从节点检查")
                    .h(px(44.0))
                    .when(self.resource_page_open(), |button| {
                        button.track_focus(&self.resource_inspect_focus)
                    })
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .tab_index(0)
                    .focus(|button| button.border_color(palette.text))
                    .when(pending, |button| button.opacity(0.6))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.inspect_resource(reference.clone(), cx);
                    })),
            )
            .children(match inspection {
                Inspection::Ready => None,
                Inspection::Pending { .. } => {
                    Some(self.text("正在检查已安装资源…", 13.0, palette.muted))
                }
                Inspection::Finished(true) => {
                    Some(self.text("节点已完成检查。", 13.0, palette.text))
                }
                Inspection::Finished(false) => {
                    Some(self.text("节点未确认检查结果。可重新检查。", 13.0, palette.danger))
                }
            })
    }
}

fn resource_matches(resource: &NativeResource, query: &str) -> bool {
    query.is_empty()
        || resource.name.to_lowercase().contains(query)
        || resource.facts["description"]
            .as_str()
            .is_some_and(|description| description.to_lowercase().contains(query))
}

fn resource_caption(resource: &NativeResource, catalog: &NativeCatalog) -> String {
    let origin = installation_origin(resource, catalog);
    resource.facts["version"]
        .as_str()
        .filter(|version| !version.is_empty())
        .map_or_else(
            || origin.to_owned(),
            |version| format!("{origin} · {version}"),
        )
}
