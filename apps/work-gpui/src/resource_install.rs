//! Install a skill or plugin from a source on the execution node.

use gpui::{Context, Div, FontWeight, Stateful, Window, div, point, prelude::*, px};
use work_client::ClientError;
use work_core::api::{NativeCatalog, NativeKind, NativeManagement, NativeReceipt, NativeScope};

use super::{ResourceTab, Shell};

#[derive(Default)]
pub(super) struct Activity {
    pending: Option<(Project, gpui::Task<()>)>,
    notice: Option<(Target, Outcome)>,
    uncertain: Vec<Target>,
}

#[derive(Clone, PartialEq, Eq)]
struct Project {
    node: String,
    id: String,
}

#[derive(Clone)]
struct Target {
    project: Project,
    scope: NativeScope,
    kind: NativeKind,
    name: String,
}

impl Target {
    fn visible_from(&self, project: &Project) -> bool {
        self.project.node == project.node
            && (self.scope == NativeScope::Global || self.project.id == project.id)
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.visible_from(&other.project)
            && self.scope == other.scope
            && self.kind == other.kind
            && self.name.eq_ignore_ascii_case(&other.name)
    }

    fn catalog_entry<'a>(&self, catalog: &'a NativeCatalog) -> Option<&'a serde_json::Value> {
        catalog.resources.iter().find_map(|entry| {
            (entry.kind == self.kind
                && entry.scope == self.scope
                && entry.name.eq_ignore_ascii_case(&self.name))
            .then_some(&entry.facts)
        })
    }

    fn confirmed(&self, receipt: &NativeReceipt, catalog: &NativeCatalog) -> bool {
        let facts = &receipt.observation["data"]["resource"];
        receipt.success
            && receipt.observation["data"]["outcome"] == "installed"
            && facts["recordedDigest"].as_str().is_some_and(|digest| {
                self.catalog_entry(catalog).is_some_and(|entry| {
                    facts["name"] == self.name
                        && entry["name"] == facts["name"]
                        && entry["kind"] == facts["kind"]
                        && entry["scope"] == facts["scope"]
                        && entry["ownership"] == "resource"
                        && entry["installed"] == true
                        && entry["recordedDigest"] == digest
                        && entry["observed"]["sha256"] == digest
                        && entry["source"] == facts["source"]
                        && entry["version"] == facts["version"]
                })
            })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Confirmed,
    Rejected,
    Unconfirmed,
    Visible,
}

impl Outcome {
    fn message(self) -> &'static str {
        match self {
            Self::Confirmed => "已安装，节点目录已确认。",
            Self::Rejected => "节点拒绝安装。请核对来源路径、名称及安装范围。",
            Self::Unconfirmed => "安装结果待核对。刷新可查看节点目录。",
            Self::Visible => "节点目录已显示同名资源，可查看详情核对来源和版本。",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Source,
    Version,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "资源名称",
            Self::Source => "节点来源路径",
            Self::Version => "版本（可选）",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

pub(super) struct Form {
    project: Project,
    kind: NativeKind,
    scope: NativeScope,
    values: [String; 3],
    field: Field,
    query: String,
    error: Option<&'static str>,
}

impl Shell {
    fn resource_project(&self) -> Option<Project> {
        self.node.as_ref()?;
        let project = self.snapshot.projects.get(self.project?)?;
        Some(Project {
            node: self
                .snapshot
                .node_id
                .clone()
                .unwrap_or_else(|| self.snapshot.node.clone()),
            id: project.id.clone(),
        })
    }

    pub(super) fn accept_resource_catalog(&mut self, result: Result<NativeCatalog, ()>) {
        if let (Ok(catalog), Some(project)) = (&result, self.resource_project()) {
            let activity = &mut self.resource_settings.install;
            let visible = |target: &Target| {
                target.visible_from(&project) && target.catalog_entry(catalog).is_some()
            };
            activity.uncertain.retain(|target| !visible(target));
            if let Some((target, outcome)) = &mut activity.notice
                && *outcome == Outcome::Unconfirmed
                && visible(target)
            {
                *outcome = Outcome::Visible;
            }
        }
        self.resource_settings.catalog = Some(result);
    }

    pub(in crate::shell) fn resource_install_field_label(&self) -> Option<&'static str> {
        self.resource_settings
            .install_form
            .as_ref()
            .map(|form| form.field.label())
    }

    pub(in crate::shell) fn finish_resource_install_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.resource_settings.install_form.as_ref() else {
            return;
        };
        match form.field {
            Field::Name => self.edit_resource_install_field(Field::Source, window, cx),
            Field::Source => self.edit_resource_install_field(Field::Version, window, cx),
            Field::Version => {}
        }
    }

    fn edit_resource_install_field(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.resource_settings.install_form.as_mut() else {
            return;
        };
        if !self.search.buffer.is_committed() || form.field == field {
            return;
        }
        form.values[form.field.index()] = self.search.buffer.text.clone();
        form.field = field;
        form.error = None;
        self.search.clear();
        self.search
            .buffer
            .replace(None, &form.values[field.index()]);
        self.search.buffer.select_all();
        self.search.focus.focus(window);
        cx.notify();
    }

    fn begin_resource_install(
        &mut self,
        kind: NativeKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.resource_project() else {
            return;
        };
        if self.resource_page.and_then(ResourceTab::kind) != Some(kind)
            || !self.can_install_resource()
            || !self.search.buffer.is_committed()
        {
            return;
        }
        self.resource_settings.install_form = Some(Form {
            project,
            kind,
            scope: NativeScope::Project,
            values: Default::default(),
            field: Field::Name,
            query: self.search.buffer.text.clone(),
            error: None,
        });
        self.search.clear();
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.search.focus.focus(window);
        cx.notify();
    }

    pub(super) fn cancel_resource_install(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.resource_settings.install_form.take() else {
            return false;
        };
        self.search.clear();
        self.search.buffer.replace(None, &form.query);
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.search.focus.focus(window);
        cx.notify();
        true
    }

    fn can_install_resource(&self) -> bool {
        matches!(self.resource_settings.catalog, Some(Ok(_)))
            && self.resource_settings.install.pending.is_none()
    }

    fn submit_resource_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.resource_project() else {
            return;
        };
        let panel = &mut self.resource_settings;
        let (Some(form), Some(Ok(catalog))) = (&mut panel.install_form, &panel.catalog) else {
            return;
        };
        if form.project != project
            || panel.install.pending.is_some()
            || !self.search.buffer.is_committed()
        {
            return;
        }
        form.values[form.field.index()] = self.search.buffer.text.clone();
        let [name, source, version] = form.values.each_ref().map(|value| value.trim());
        let target = Target {
            project: project.clone(),
            scope: form.scope,
            kind: form.kind,
            name: name.to_owned(),
        };
        form.error = if target.catalog_entry(catalog).is_some() {
            Some("该范围已有同名资源，请更换名称或范围。")
        } else if panel
            .install
            .uncertain
            .iter()
            .any(|prior| prior.same_slot(&target))
        {
            Some("此资源的上次安装仍待节点确认，请返回目录刷新。")
        } else if !valid_install_name(name) {
            Some("名称须以 ASCII 字母或数字开头，可使用点、下划线和连字符，最多 96 字符。")
        } else if source.is_empty()
            || source.chars().any(char::is_control)
            || version.chars().any(char::is_control)
        {
            Some("请填写节点上的来源路径。路径和版本须为单行文本。")
        } else {
            None
        };
        if form.error.is_some() {
            self.resource_settings
                .scroll
                .set_offset(point(px(0.0), px(0.0)));
            cx.notify();
            return;
        }
        let Some(node) = self.node.as_ref() else {
            return;
        };
        let request = NativeManagement::Install {
            scope: target.scope,
            kind: target.kind,
            name: target.name.clone(),
            source: source.to_owned(),
            version: (!version.is_empty()).then(|| version.to_owned()),
            trusted: true,
        };
        let write = node.install_native_resource(project.id.clone(), request);
        self.cancel_resource_install(window, cx);
        self.resource_settings.install.notice = None;
        let task = cx.spawn(async move |this, cx| {
            let result = write.await.ok();
            let _ = this.update(cx, |shell, cx| {
                shell.finish_resource_install(target, result, cx)
            });
        });
        self.resource_settings.install.pending = Some((project, task));
        cx.notify();
    }

    fn finish_resource_install(
        &mut self,
        target: Target,
        result: Option<crate::data::ResourceInstall>,
        cx: &mut Context<Self>,
    ) {
        let outcome = match result.as_ref().map(|result| &result.receipt) {
            Some(Ok(receipt)) if !receipt.success => Outcome::Rejected,
            Some(Ok(receipt))
                if result
                    .as_ref()
                    .and_then(|result| result.observed.as_ref())
                    .is_some_and(|catalog| target.confirmed(receipt, catalog)) =>
            {
                Outcome::Confirmed
            }
            Some(Err(ClientError::Api { status, .. }))
                if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 409 | 422) =>
            {
                Outcome::Rejected
            }
            _ => Outcome::Unconfirmed,
        };
        let current = self.resource_project();
        let same_project = current.as_ref() == Some(&target.project);
        let visible = current
            .as_ref()
            .is_some_and(|project| target.visible_from(project));
        let activity = &mut self.resource_settings.install;
        activity.pending = None;
        if outcome == Outcome::Unconfirmed {
            activity.uncertain.push(target.clone());
        }
        // A concurrent refresh might have captured the catalog before the write.
        if same_project {
            self.resource_settings.pending = None;
            self.resource_settings.detail = None;
            self.accept_resource_catalog(result.and_then(|result| result.observed).ok_or(()));
        } else if visible
            && (self.resource_settings.catalog.is_some()
                || self.resource_settings.pending.is_some())
        {
            self.resource_settings.pending = None;
            self.read_resource_catalog(cx);
        }
        // A missing receipt remains unconfirmed until the next explicit catalog read.
        self.resource_settings.install.notice = Some((target, outcome));
        cx.notify();
    }

    pub(super) fn resource_install_button(
        &self,
        kind: NativeKind,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.can_install_resource().then(|| {
            self.install_action("install-resource", install_label(kind))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.begin_resource_install(kind, window, cx)
                }))
        })
    }

    pub(super) fn resource_install_notice(&self, tab: ResourceTab) -> Option<Div> {
        let project = self.resource_project()?;
        let activity = &self.resource_settings.install;
        let message = if activity
            .pending
            .as_ref()
            .is_some_and(|(owner, _)| *owner == project)
        {
            "正在安装并读取节点目录…"
        } else {
            let (target, outcome) = activity.notice.as_ref()?;
            if !target.visible_from(&project) || Some(target.kind) != tab.kind() {
                return None;
            }
            outcome.message()
        };
        Some(self.text(message, 13.0, self.palette.muted).mb_3())
    }

    pub(super) fn resource_install_form(&self, form: &Form, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .max_w(px(620.0))
            .child(
                self.text(install_label(form.kind), 22.0, palette.text)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(self.text(
                match form.kind {
                    NativeKind::Skill => {
                        "填写包含 SKILL.md 的目录，可用项目相对路径或节点授权的绝对路径。"
                    }
                    NativeKind::Extension => {
                        "填写已构建的原生库，可用项目相对路径或节点授权的绝对路径。"
                    }
                    NativeKind::Package => "填写包目录，可用项目相对路径或节点授权的绝对路径。",
                },
                13.0,
                palette.muted,
            ))
            .when_some(form.error, |body, error| {
                body.child(self.text(error, 13.0, palette.danger))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.text("安装范围", 13.0, palette.text)
                            .w(px(100.0))
                            .flex_none(),
                    )
                    .children(
                        [
                            (NativeScope::Project, "当前项目"),
                            (NativeScope::Global, "节点全局"),
                        ]
                        .map(|(scope, label)| {
                            self.install_action(label, label)
                                .when(scope == form.scope, |button| {
                                    button.bg(palette.subtle).border_color(palette.strong_line)
                                })
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    if let Some(form) =
                                        shell.resource_settings.install_form.as_mut()
                                    {
                                        form.scope = scope;
                                        form.error = None;
                                        cx.notify();
                                    }
                                }))
                        }),
                    ),
            )
            .children(
                [Field::Name, Field::Source, Field::Version]
                    .map(|field| self.resource_install_field(form, field, cx)),
            )
            .child(self.text("仅在确认信任此来源时安装。", 13.0, palette.muted))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.install_action("cancel-resource-install", "取消")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.cancel_resource_install(window, cx);
                            })),
                    )
                    .child(
                        self.install_action("confirm-resource-install", "信任并安装")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.submit_resource_install(window, cx)
                            })),
                    ),
            )
    }

    fn resource_install_field(&self, form: &Form, field: Field, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let row = div().flex().items_center().gap_2().child(
            self.text(field.label(), 13.0, palette.text)
                .w(px(100.0))
                .flex_none(),
        );
        if form.field == field {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .h(px(44.0))
                    .px_2()
                    .border_1()
                    .border_color(palette.strong_line)
                    .rounded(px(6.0))
                    .child(self.search_input_element(cx)),
            )
        } else {
            let value = &form.values[field.index()];
            row.child(
                self.install_action(field.label(), "")
                    .flex_1()
                    .min_w_0()
                    .justify_start()
                    .child(
                        self.text(
                            if value.is_empty() {
                                "未填写".into()
                            } else {
                                value.clone()
                            },
                            13.0,
                            palette.text,
                        )
                        .truncate(),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.edit_resource_install_field(field, window, cx)
                    })),
            )
        }
    }

    fn install_action(&self, id: &'static str, label: &'static str) -> Stateful<Div> {
        self.button(id, label)
            .h(px(44.0))
            .px_3()
            .tab_index(0)
            .border_1()
            .border_color(self.palette.line)
            .focus(|button| button.border_color(self.palette.strong_line))
    }
}

fn valid_install_name(name: &str) -> bool {
    name.len() <= 96
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !name.ends_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn install_label(kind: NativeKind) -> &'static str {
    match kind {
        NativeKind::Skill => "安装技能",
        NativeKind::Extension => "安装插件",
        NativeKind::Package => "安装包",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use work_core::api::NativeResource;

    #[test]
    fn an_unconfirmed_global_install_tracks_the_same_slot_from_another_project() {
        let mut first = Target {
            project: Project {
                node: "node".into(),
                id: "alpha".into(),
            },
            scope: NativeScope::Global,
            kind: NativeKind::Skill,
            name: "Example".into(),
        };
        let mut second = first.clone();
        second.project.id = "beta".into();
        second.name = "example".into();
        assert!(first.same_slot(&second));
        second.project.node = "other".into();
        assert!(!first.same_slot(&second));
        second.project.node = first.project.node.clone();
        first.scope = NativeScope::Project;
        second.scope = NativeScope::Project;
        assert!(!first.same_slot(&second));
    }

    #[test]
    fn a_success_receipt_requires_matching_observed_content_and_source() {
        let target = Target {
            project: Project {
                node: "node".into(),
                id: "alpha".into(),
            },
            scope: NativeScope::Project,
            kind: NativeKind::Skill,
            name: "example".into(),
        };
        let facts = json!({
            "name": "example", "kind": "skill", "scope": "project", "source": "/project/source",
            "version": "1.2.3", "ownership": "resource", "installed": true,
            "recordedDigest": "installed", "observed": { "sha256": "installed" }
        });
        let receipt = NativeReceipt {
            success: true,
            diagnostics: String::new(),
            observation: json!({ "data": { "outcome": "installed", "resource": facts } }),
        };
        let mut catalog = NativeCatalog {
            profile: serde_json::Value::Null,
            resources: vec![NativeResource {
                reference: "installed".into(),
                scope: target.scope,
                kind: target.kind,
                name: target.name.clone(),
                facts,
            }],
            diagnostics: Vec::new(),
        };
        assert!(target.confirmed(&receipt, &catalog));
        catalog.resources[0].facts["observed"]["sha256"] = json!("changed");
        assert!(!target.confirmed(&receipt, &catalog));
        catalog.resources[0].facts["observed"]["sha256"] = json!("installed");
        catalog.resources[0].facts["source"] = json!("/project/other");
        assert!(!target.confirmed(&receipt, &catalog));
    }
}
