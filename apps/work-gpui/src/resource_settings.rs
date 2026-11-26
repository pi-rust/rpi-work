//! Read the selected project's resource catalog from its node.

use gpui::{Context, Div, FontWeight, Task, div, prelude::*};
use work_core::api::{NativeCatalog, NativeKind, NativeResource};
use work_viewmodel::resources::catalog::{display_name, installation_origin, installed_resources};

use super::Shell;

#[derive(Default)]
pub(super) struct Panel {
    pending: Option<Task<()>>,
    catalog: Option<Result<NativeCatalog, ()>>,
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
                        .children(rows.map(|resource| self.resource_row(resource, catalog)))
                }))
            }
        }
    }

    fn resource_row(&self, resource: &NativeResource, catalog: &NativeCatalog) -> Div {
        let palette = self.palette;
        let origin = installation_origin(resource, catalog);
        let detail = resource.facts["version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .map_or_else(
                || origin.to_owned(),
                |version| format!("{origin} · {version}"),
            );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .border_t_1()
            .border_color(palette.line)
            .py_2()
            .child(self.text(display_name(&resource.name), 14.0, palette.text))
            .child(self.text(detail, 12.0, palette.muted))
    }
}
