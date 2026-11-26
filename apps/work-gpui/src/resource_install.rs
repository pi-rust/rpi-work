//! Manage installed resources on the execution node.

use gpui::{Context, Div, FontWeight, Stateful, Window, div, point, prelude::*, px};
use work_client::ClientError;
use work_core::api::{
    NativeCatalog, NativeKind, NativeManagement, NativeReceipt, NativeResource, NativeScope,
};

use super::{ResourceTab, Shell};

#[derive(Default)]
pub(super) struct Activity {
    pending: Option<(Target, gpui::Task<()>)>,
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
    action: Action,
    previous: Option<NativeResource>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Install,
    Update,
    Reinstall,
    Remove,
}

impl Action {
    fn label(self, kind: NativeKind) -> &'static str {
        match self {
            Self::Install => install_label(kind),
            Self::Update => "更新",
            Self::Reinstall => "重装",
            Self::Remove => "卸载",
        }
    }

    fn confirm(self) -> &'static str {
        match self {
            Self::Install => "信任并安装",
            Self::Update => "信任并更新",
            Self::Reinstall => "信任并重装",
            Self::Remove => "确认卸载",
        }
    }

    fn receipt_name(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Update => "update",
            Self::Reinstall => "reinstall",
            Self::Remove => "remove",
        }
    }
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

    fn changed_in(&self, catalog: &NativeCatalog) -> bool {
        let entry = self.catalog_entry(catalog);
        if self.action == Action::Remove {
            return entry.is_none();
        }
        entry.is_some_and(|entry| {
            self.previous.as_ref().is_none_or(|previous| {
                ["recordedDigest", "source", "version", "installed"]
                    .iter()
                    .any(|&field| entry[field] != previous.facts[field])
            })
        })
    }

    fn confirmed(&self, receipt: &NativeReceipt, catalog: &NativeCatalog) -> bool {
        let observation = &receipt.observation;
        let data = &observation["data"];
        if !receipt.success
            || observation["operation"] != self.action.receipt_name()
            || observation["name"] != self.name
            || observation["kind"] != serde_json::json!(self.kind)
            || observation["scope"] != serde_json::json!(self.scope)
        {
            return false;
        }
        let outcome = data["outcome"].as_str();
        match self.action {
            Action::Remove => return outcome == Some("removed") && self.changed_in(catalog),
            Action::Install if outcome != Some("installed") => return false,
            Action::Update | Action::Reinstall
                if !matches!(outcome, Some("replaced" | "unchanged")) =>
            {
                return false;
            }
            _ => {}
        }
        let facts = &data["resource"];
        facts["recordedDigest"].as_str().is_some_and(|digest| {
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
    Referenced,
    Unconfirmed,
    Visible,
}

impl Outcome {
    fn message(self, target: &Target) -> String {
        let action = target.action.label(target.kind);
        match self {
            Self::Confirmed => format!("{action}完成，节点目录已确认。"),
            Self::Rejected => format!("节点拒绝{action}。请检查资源详情和节点来源。"),
            Self::Referenced => "节点拒绝修改。此资源仍被已保存的项目或会话引用。".into(),
            Self::Unconfirmed => format!("{action}结果待核对。刷新可查看节点目录。"),
            Self::Visible if target.action == Action::Remove => "节点目录已无此资源。".into(),
            Self::Visible => "节点目录已有变更，可查看详情核对来源和版本。".into(),
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
    action: Action,
    previous: Option<NativeResource>,
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
            let visible =
                |target: &Target| target.visible_from(&project) && target.changed_in(catalog);
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
        if form.action == Action::Remove {
            return;
        }
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
        let active_kind = if self.resource_settings.configuration_form.is_some() {
            Some(NativeKind::Package)
        } else {
            self.resource_page.and_then(ResourceTab::kind)
        };
        if active_kind != Some(kind)
            || self.resource_settings.install_form.is_some()
            || !self.can_install_resource()
            || !self.search.buffer.is_committed()
        {
            return;
        }
        self.resource_settings.install_form = Some(Form {
            project,
            kind,
            scope: NativeScope::Project,
            action: Action::Install,
            previous: None,
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

    fn begin_resource_change(
        &mut self,
        resource: NativeResource,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_change_resource(&resource) || !self.search.buffer.is_committed() {
            return;
        }
        let Some(project) = self.resource_project() else {
            return;
        };
        let fields = ["source", "version"].map(|field| {
            resource.facts[field]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        });
        self.search.focus.focus(window);
        let query = self.search.buffer.text.clone();
        self.search.clear();
        self.search.buffer.replace(None, &fields[0]);
        self.search.buffer.select_all();
        self.resource_settings.install_form = Some(Form {
            project,
            kind: resource.kind,
            scope: resource.scope,
            action,
            values: [resource.name.clone(), fields[0].clone(), fields[1].clone()],
            previous: Some(resource),
            field: Field::Source,
            query,
            error: None,
        });
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    fn can_change_resource(&self, resource: &NativeResource) -> bool {
        self.resource_page.and_then(ResourceTab::kind) == Some(resource.kind)
            && self.resource_settings.install_form.is_none()
            && self.resource_settings.install.pending.is_none()
            && resource.facts["ownership"] == "resource"
            && resource.facts["installed"] == true
            && matches!(&self.resource_settings.catalog, Some(Ok(catalog)) if catalog.protects_saved_references)
    }

    pub(super) fn resource_change_buttons(
        &self,
        resource: &NativeResource,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        self.can_change_resource(resource).then(|| {
            div().flex().flex_wrap().gap_2().children(
                [Action::Update, Action::Reinstall, Action::Remove].map(|action| {
                    let label = action.label(resource.kind);
                    let resource = resource.clone();
                    self.install_action(label, label).on_click(cx.listener(
                        move |shell, _, window, cx| {
                            shell.begin_resource_change(resource.clone(), action, window, cx);
                        },
                    ))
                }),
            )
        })
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
        if let Some(form) = &self.resource_settings.configuration_form {
            form.focus_back(window);
        } else {
            self.search.focus.focus(window);
        }
        cx.notify();
        true
    }

    pub(super) fn resource_install_in_progress(&self) -> bool {
        self.resource_settings.install.pending.is_some()
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
            action: form.action,
            previous: form.previous.clone(),
        };
        form.error = if form.action == Action::Install && target.catalog_entry(catalog).is_some() {
            Some("该范围已有同名资源，请更换名称或范围。")
        } else if panel
            .install
            .uncertain
            .iter()
            .any(|prior| prior.same_slot(&target))
        {
            Some("此资源的上次操作仍待节点确认，请返回目录刷新。")
        } else if form.previous.as_ref().is_some_and(|previous| {
            !catalog.protects_saved_references
                || !catalog.resources.iter().any(|entry| {
                    entry.reference == previous.reference
                        && entry.facts["source"] == previous.facts["source"]
                        && entry.facts["version"] == previous.facts["version"]
                        && entry.facts["observed"]["sha256"] == previous.facts["observed"]["sha256"]
                })
        }) {
            Some("节点目录已更新，请返回目录重新打开资源。")
        } else if form.action == Action::Install && !valid_install_name(name) {
            Some("名称须以 ASCII 字母或数字开头，可使用点、下划线和连字符，最多 96 字符。")
        } else if form.action != Action::Remove
            && (source.is_empty()
                || source.chars().any(char::is_control)
                || version.chars().any(char::is_control))
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
        let reference = target
            .previous
            .as_ref()
            .map(|entry| entry.reference.clone())
            .unwrap_or_default();
        let source = source.to_owned();
        let version = (!version.is_empty()).then(|| version.to_owned());
        let request = match form.action {
            Action::Install => NativeManagement::Install {
                scope: target.scope,
                kind: target.kind,
                name: target.name.clone(),
                source,
                version,
                trusted: true,
            },
            Action::Update => NativeManagement::Update {
                reference,
                source,
                version,
                trusted: true,
            },
            Action::Reinstall => NativeManagement::Reinstall {
                reference,
                source,
                version,
                trusted: true,
            },
            Action::Remove => NativeManagement::Remove { reference },
        };
        let write = node.manage_native_resource(project.id, request);
        self.cancel_resource_install(window, cx);
        self.resource_settings.install.notice = None;
        let pending_target = target.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = write.await.ok();
            let _ = this.update(cx, |shell, cx| {
                shell.finish_resource_install(target, result, cx)
            });
        });
        self.resource_settings.install.pending = Some((pending_target, task));
        cx.notify();
    }

    fn finish_resource_install(
        &mut self,
        target: Target,
        result: Option<crate::data::ResourceManagement>,
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
            Some(Err(error))
                if error
                    .to_string()
                    .contains("resource is referenced by a saved ") =>
            {
                Outcome::Referenced
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
        self.resource_settings.install.pending = None;
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
        let activity = &mut self.resource_settings.install;
        if outcome == Outcome::Unconfirmed {
            activity.uncertain.push(target.clone());
        }
        activity.notice = Some((target, outcome));
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

    pub(super) fn resource_install_notice(&self, kind: Option<NativeKind>) -> Option<Div> {
        let project = self.resource_project()?;
        let activity = &self.resource_settings.install;
        let message = if activity
            .pending
            .as_ref()
            .is_some_and(|(target, _)| target.visible_from(&project))
        {
            "正在提交资源操作并读取节点目录…".into()
        } else {
            let (target, outcome) = activity.notice.as_ref()?;
            if !target.visible_from(&project) || Some(target.kind) != kind {
                return None;
            }
            outcome.message(target)
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
                self.text(form.action.label(form.kind), 22.0, palette.text)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .when(form.action != Action::Remove, |body| {
                body.child(self.text(
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
            })
            .when_some(form.previous.as_ref(), |body, resource| {
                let scope = match resource.scope {
                    NativeScope::Project => "当前项目",
                    NativeScope::Global => "节点全局",
                };
                body.child(self.text(format!("{} · {scope}", resource.name), 14.0, palette.text))
            })
            .when_some(form.error, |body, error| {
                body.child(self.text(error, 13.0, palette.danger))
            })
            .when(form.action == Action::Install, |body| {
                body.child(
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
            })
            .when(form.action == Action::Install, |body| {
                body.child(self.resource_install_field(form, Field::Name, cx))
            })
            .when(form.action != Action::Remove, |body| {
                body.children(
                    [Field::Source, Field::Version]
                        .map(|field| self.resource_install_field(form, field, cx)),
                )
                .child(self.text(
                    "请确认信任填写的节点来源。",
                    13.0,
                    palette.muted,
                ))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.install_action("cancel-resource-install", "取消")
                            .when(form.action == Action::Remove, |button| {
                                button.track_focus(&self.search.focus)
                            })
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.cancel_resource_install(window, cx);
                            })),
                    )
                    .child(
                        self.install_action("confirm-resource-install", form.action.confirm())
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
            action: Action::Install,
            previous: None,
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
    fn a_lost_receipt_requires_a_change_in_the_same_catalog_slot() {
        let resource = NativeResource {
            reference: "original".into(),
            scope: NativeScope::Global,
            kind: NativeKind::Extension,
            name: "example".into(),
            facts: json!({"recordedDigest":"original","version":"1.0.0","installed":true}),
        };
        let mut target = Target {
            project: Project {
                node: "node".into(),
                id: "alpha".into(),
            },
            scope: resource.scope,
            kind: resource.kind,
            name: resource.name.clone(),
            action: Action::Reinstall,
            previous: Some(resource.clone()),
        };
        let mut catalog = NativeCatalog {
            protects_saved_references: true,
            profile: serde_json::Value::Null,
            resources: vec![resource],
            diagnostics: Vec::new(),
        };
        assert!(!target.changed_in(&catalog));
        catalog.resources[0].facts["version"] = json!("2.0.0");
        assert!(target.changed_in(&catalog));
        target.action = Action::Remove;
        assert!(!target.changed_in(&catalog));
        catalog.resources[0].scope = NativeScope::Project;
        assert!(target.changed_in(&catalog));
        let mut receipt = NativeReceipt {
            success: true,
            diagnostics: String::new(),
            observation: json!({"operation":"remove","scope":"global","kind":"extension","name":"example","data":{"outcome":"absent"}}),
        };
        assert!(!target.confirmed(&receipt, &catalog));
        receipt.observation["data"]["outcome"] = json!("removed");
        assert!(target.confirmed(&receipt, &catalog));
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
            action: Action::Install,
            previous: None,
        };
        let facts = json!({
            "name": "example", "kind": "skill", "scope": "project", "source": "/project/source",
            "version": "1.2.3", "ownership": "resource", "installed": true,
            "recordedDigest": "installed", "observed": { "sha256": "installed" }
        });
        let receipt = NativeReceipt {
            success: true,
            diagnostics: String::new(),
            observation: json!({ "operation": "install", "name": "example", "scope": "project", "kind": "skill", "data": { "outcome": "installed", "resource": facts } }),
        };
        let mut catalog = NativeCatalog {
            protects_saved_references: false,
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
