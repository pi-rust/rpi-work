//! Browse and inspect the selected project's installed resources.

use gpui::{Context, Div, FontWeight, Task, div, prelude::*, px};
use work_core::api::{NativeCatalog, NativeKind, NativeResource};
use work_viewmodel::resources::catalog::{display_name, installation_origin, installed_resources};

use super::Shell;

#[derive(Default)]
pub(super) struct Panel {
    pending: Option<Task<()>>,
    catalog: Option<Result<NativeCatalog, ()>>,
    detail: Option<Detail>,
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

    fn inspect_resource(&mut self, cx: &mut Context<Self>) {
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
                .child(
                    self.button("refresh-native-catalog", "刷新")
                        .border_1()
                        .border_color(palette.line)
                        .tab_index(0)
                        .focus(|button| button.bg(palette.hover))
                        .when(pending, |button| button.opacity(0.6))
                        .on_click(cx.listener(|shell, _, _, cx| shell.read_resource_catalog(cx))),
                ),
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
        let origin = installation_origin(resource, catalog);
        let detail = resource.facts["version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .map_or_else(
                || origin.to_owned(),
                |version| format!("{origin} · {version}"),
            );
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
                            .child(self.text(detail, 12.0, palette.muted)),
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
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .tab_index(0)
                    .focus(|button| button.border_color(palette.text))
                    .when(pending, |button| button.opacity(0.6))
                    .on_click(cx.listener(|shell, _, _, cx| shell.inspect_resource(cx))),
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
