//! Project defaults for newly created sessions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    Context, Div, FocusHandle, FontWeight, Stateful, Task, Window, div, point, prelude::*, px,
};
use work_client::ClientError;
use work_core::api::{
    NativeApply, NativeCatalog, NativeKind, NativeOperationStatus, NativeProjectResources,
    NativeResource, NativeScope, NativeSelection, NativeTools,
};
use work_viewmodel::resources::catalog::display_name;

use super::Shell;
use crate::data::ProjectResourceWrite;

#[derive(Default)]
pub(super) struct Activity {
    pending: Option<Task<()>>,
    attempts: HashMap<String, Attempt>,
}

struct Attempt {
    request: NativeApply,
    outcome: Outcome,
}

#[derive(PartialEq, Eq)]
enum Outcome {
    Confirmed,
    Observed,
    Rejected,
    Superseded,
    Unconfirmed,
}

impl Attempt {
    fn reconcile(&mut self, observed: &NativeProjectResources) {
        if self.outcome != Outcome::Unconfirmed
            || observed.revision <= self.request.expected_revision
        {
            return;
        }
        self.outcome = if observed.selection.as_ref() == Some(&self.request.target) {
            Outcome::Observed
        } else {
            Outcome::Superseded
        };
    }

    fn notice(&self) -> &'static str {
        match self.outcome {
            Outcome::Confirmed => "项目资源配置已保存并回读确认。",
            Outcome::Observed => "节点当前配置与提交的选择一致。",
            Outcome::Rejected => "节点拒绝保存。请核对当前版本和资源选择。",
            Outcome::Superseded => "节点已有更新的配置，请核对后再编辑。",
            Outcome::Unconfirmed => "保存结果待核对。刷新只读取节点配置。",
        }
    }
}

pub(super) struct Form {
    project: String,
    name: String,
    back: FocusHandle,
    kind: NativeKind,
    pending: Option<Task<()>>,
    baseline: Option<NativeProjectResources>,
    draft: NativeSelection,
}

impl Form {
    pub(super) fn focus_back(&self, window: &mut Window) {
        self.back.focus(window);
    }

    fn accept(&mut self, state: NativeProjectResources) {
        self.draft = state.selection.clone().unwrap_or_else(empty_selection);
        self.baseline = Some(state);
    }

    fn choose(&mut self, kind: NativeKind, reference: &str) {
        if kind == NativeKind::Package {
            self.draft.main_package = reference.to_owned();
            self.draft.package_instructions = self
                .baseline
                .as_ref()
                .and_then(|base| base.selection.as_ref())
                .filter(|base| base.main_package == reference)
                .map_or_else(Vec::new, |base| base.package_instructions.clone());
        } else {
            let references = if kind == NativeKind::Skill {
                &mut self.draft.skills
            } else {
                &mut self.draft.extensions
            };
            if references.iter().any(|value| value == reference) {
                references.retain(|value| value != reference);
            } else {
                references.push(reference.to_owned());
            }
        }
    }

    fn selected(&self, kind: NativeKind) -> &[String] {
        match kind {
            NativeKind::Package => std::slice::from_ref(&self.draft.main_package),
            NativeKind::Skill => &self.draft.skills,
            NativeKind::Extension => &self.draft.extensions,
        }
    }

    fn changed(&self) -> bool {
        self.baseline
            .as_ref()
            .is_some_and(|base| base.selection.as_ref() != Some(&self.draft))
    }
}

fn empty_selection() -> NativeSelection {
    NativeSelection {
        main_package: String::new(),
        package_instructions: Vec::new(),
        skills: Vec::new(),
        extensions: Vec::new(),
        tools: NativeTools::Default {},
        supplementary_instructions: String::new(),
    }
}

fn selectable(resource: &NativeResource) -> bool {
    let facts = &resource.facts;
    facts["installed"] == true
        && facts["ownership"] == "resource"
        && facts["recordedDigest"]
            .as_str()
            .is_some_and(|digest| facts["observed"]["sha256"] == digest)
        && match resource.kind {
            NativeKind::Package => facts["selectable"]["main_package"].is_object(),
            NativeKind::Skill => facts["selectable"]["skills"][0].is_string(),
            NativeKind::Extension => facts["selectable"]["extensions"][0].is_string(),
        }
}

fn available(catalog: &NativeCatalog, kind: NativeKind, reference: &str) -> bool {
    catalog
        .resources
        .iter()
        .any(|entry| entry.kind == kind && entry.reference == reference && selectable(entry))
}

fn receipt_outcome(request: &NativeApply, result: Option<&ProjectResourceWrite>) -> Outcome {
    let Some(result) = result else {
        return Outcome::Unconfirmed;
    };
    match &result.receipt {
        Ok(receipt)
            if receipt.status == NativeOperationStatus::Applied
                && receipt.operation_id == request.operation_id
                && receipt.expected_revision == request.expected_revision
                && request.expected_revision.checked_add(1) == receipt.result_revision
                && result.observed.as_ref().is_some_and(|observed| {
                    Some(observed.revision) == receipt.result_revision
                        && observed.selection.as_ref() == Some(&request.target)
                        && observed.diagnostic.is_none()
                }) =>
        {
            Outcome::Confirmed
        }
        Err(ClientError::Api { status, .. })
            if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 409 | 413 | 422) =>
        {
            Outcome::Rejected
        }
        _ => Outcome::Unconfirmed,
    }
}

impl Shell {
    fn configuration_catalog(&self) -> Option<&NativeCatalog> {
        self.resource_settings.catalog.as_ref()?.as_ref().ok()
    }

    pub(super) fn resource_configuration_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.configuration_catalog()?;
        Some(
            self.configuration_button("project-resource-configuration", "项目资源配置")
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.open_resource_configuration(window, cx)
                })),
        )
    }

    fn open_resource_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self
            .project
            .and_then(|index| self.snapshot.projects.get(index))
        else {
            return;
        };
        if self.composer_is_marked()
            || self.resource_settings.configuration_form.is_some()
            || self.configuration_catalog().is_none()
        {
            return;
        }
        let back = cx.focus_handle().tab_stop(true);
        back.focus(window);
        self.resource_settings.configuration_form = Some(Form {
            project: project.id.clone(),
            name: project.name.clone(),
            back,
            kind: NativeKind::Package,
            pending: None,
            baseline: None,
            draft: empty_selection(),
        });
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.read_resource_configuration(cx);
    }

    pub(super) fn close_resource_configuration(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.resource_settings.configuration_form.take().is_none() {
            return false;
        }
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.search.focus.focus(window);
        cx.notify();
        true
    }

    fn read_resource_configuration(&mut self, cx: &mut Context<Self>) {
        let panel = &mut self.resource_settings;
        let (Some(form), Some(node)) = (&mut panel.configuration_form, &self.node) else {
            return;
        };
        if form.pending.is_some() || panel.configuration.pending.is_some() {
            cx.notify();
            return;
        }
        let read = node.read_project_resources(form.project.clone());
        form.pending = Some(cx.spawn(async move |this, cx| {
            let state = read.await.ok().flatten();
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.resource_settings;
                if let Some(form) = &mut panel.configuration_form {
                    form.pending = None;
                    if let Some(state) = state {
                        if let Some(attempt) = panel.configuration.attempts.get_mut(&form.project) {
                            attempt.reconcile(&state);
                        }
                        if form.baseline.as_ref() != Some(&state) {
                            form.accept(state);
                        }
                    } else {
                        form.baseline = None;
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn can_save_resource_configuration(&self, form: &Form, catalog: &NativeCatalog) -> bool {
        let activity = &self.resource_settings.configuration;
        activity.pending.is_none()
            && !self.resource_install_in_progress()
            && !activity
                .attempts
                .get(&form.project)
                .is_some_and(|entry| entry.outcome == Outcome::Unconfirmed)
            && form.pending.is_none()
            && form.changed()
            && [
                NativeKind::Package,
                NativeKind::Skill,
                NativeKind::Extension,
            ]
            .into_iter()
            .all(|kind| {
                form.selected(kind)
                    .iter()
                    .all(|reference| available(catalog, kind, reference))
            })
    }

    fn save_resource_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = &self.resource_settings;
        let (Some(form), Some(catalog), Some(node)) = (
            &panel.configuration_form,
            self.configuration_catalog(),
            &self.node,
        ) else {
            return;
        };
        if !self.can_save_resource_configuration(form, catalog) {
            return;
        }
        let Some(baseline) = &form.baseline else {
            return;
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let request = NativeApply {
            expected_revision: baseline.revision,
            operation_id: format!(
                "gpui-{}-{now}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            target: form.draft.clone(),
            supersedes_operation_id: None,
        };
        let project = form.project.clone();
        form.back.focus(window);
        self.resource_settings
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        let write = node.write_project_resources(project.clone(), request.clone());
        self.resource_settings.configuration.pending = Some(cx.spawn(async move |this, cx| {
            let result = write.await.ok();
            let outcome = receipt_outcome(&request, result.as_ref());
            let attempt = Attempt { request, outcome };
            let observed = result.and_then(|result| result.observed);
            // A known rejection with a newer baseline needs an explicit re-edit.
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.resource_settings;
                panel.configuration.pending = None;
                let mut read_other = false;
                if let Some(form) = panel.configuration_form.as_mut() {
                    if form.project == project {
                        if let Some(observed) = observed {
                            let keep_draft = attempt.outcome == Outcome::Rejected
                                && form.baseline.as_ref() == Some(&observed);
                            if !keep_draft {
                                form.accept(observed);
                            }
                        } else {
                            form.baseline = None;
                        }
                    } else if form.baseline.is_none() && form.pending.is_none() {
                        read_other = true;
                    }
                }
                panel.configuration.attempts.insert(project, attempt);
                if read_other {
                    shell.read_resource_configuration(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn resource_configuration_form(&self, form: &Form, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let activity = &self.resource_settings.configuration;
        let setup = div()
            .flex()
            .flex_col()
            .gap_3()
            .max_w(px(760.0))
            .child(
                self.text("项目资源配置", 22.0, palette.text)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(self.text(
                format!("为「{}」的新会话选择主包、技能和插件。", form.name),
                13.0,
                palette.muted,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        self.configuration_button("close-project-resources", "返回资源目录")
                            .track_focus(&form.back)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_resource_configuration(window, cx);
                            })),
                    )
                    .when(
                        form.pending.is_none() && activity.pending.is_none(),
                        |body| {
                            body.child(
                                self.configuration_button(
                                    "refresh-project-resources",
                                    "读取节点配置",
                                )
                                .on_click(cx.listener(
                                    |shell, _, window, cx| {
                                        if let Some(form) =
                                            &shell.resource_settings.configuration_form
                                        {
                                            form.back.focus(window);
                                        }
                                        shell.read_resource_configuration(cx);
                                        shell.read_resource_catalog(cx);
                                    },
                                )),
                            )
                        },
                    ),
            );
        if form.pending.is_some() || activity.pending.is_some() {
            let message = if activity.pending.is_some() {
                "正在保存并回读项目资源配置…"
            } else {
                "正在读取项目资源配置…"
            };
            return setup.child(self.text(message, 13.0, palette.muted));
        }
        let setup = setup.when_some(activity.attempts.get(&form.project), |body, attempt| {
            body.child(self.text(attempt.notice(), 13.0, palette.muted))
        });
        if self.resource_settings.pending.is_some() {
            return setup.child(self.text("正在读取节点资源目录…", 13.0, palette.muted));
        }
        let (Some(base), Some(catalog)) = (form.baseline.as_ref(), self.configuration_catalog())
        else {
            return setup.child(self.text(
                "无法读取项目资源配置。可重新读取。",
                13.0,
                palette.danger,
            ));
        };
        let state = if base.selection.is_none() {
            "节点尚未保存显式配置。".into()
        } else {
            format!("保存版本 {}", base.revision)
        };
        let setup = setup
            .child(self.text(state, 13.0, palette.muted))
            .when(base.diagnostic.is_some(), |body| {
                body.child(self.text("节点报告已保存配置包含不可用的资源。", 13.0, palette.danger))
            })
            .when(
                base.selection.as_ref().is_some_and(|selection| {
                    selection.main_package != form.draft.main_package
                        && !selection.package_instructions.is_empty()
                }),
                |body| body.child(self.text("保存后将清除原主包的专属指引。", 13.0, palette.muted)),
            )
            .child(
                div().flex().flex_wrap().gap_2().children(
                    [
                        (NativeKind::Package, "主包"),
                        (NativeKind::Skill, "技能"),
                        (NativeKind::Extension, "插件"),
                    ]
                    .map(|(kind, label)| {
                        self.configuration_button(label, label)
                            .when(form.kind == kind, |button| {
                                button.bg(palette.subtle).border_color(palette.strong_line)
                            })
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                if let Some(form) =
                                    shell.resource_settings.configuration_form.as_mut()
                                {
                                    form.kind = kind;
                                    cx.notify();
                                }
                            }))
                    }),
                ),
            );
        let setup = setup.when(form.kind == NativeKind::Package, |body| {
            body.children(self.resource_install_notice(Some(NativeKind::Package)))
        });
        let selected = form.selected(form.kind);
        let entries = catalog
            .resources
            .iter()
            .filter(|entry| entry.kind == form.kind && selectable(entry));
        let missing = selected
            .iter()
            .filter(|reference| !reference.is_empty() && !available(catalog, form.kind, reference));
        let group = div()
            .flex()
            .flex_col()
            .gap_1()
            .when(entries.clone().next().is_none(), |list| {
                list.child(self.text("此类别没有可选的已安装资源。", 13.0, palette.muted))
            })
            .children(entries.map(|entry| {
                let scope = match entry.scope {
                    NativeScope::Project => "项目",
                    NativeScope::Global => "全局",
                };
                let caption = entry.facts["version"].as_str().map_or_else(
                    || scope.to_owned(),
                    |version| format!("{scope} · {version}"),
                );
                self.configuration_row(
                    form.kind,
                    entry.reference.clone(),
                    display_name(&entry.name),
                    caption,
                    selected.contains(&entry.reference),
                    cx,
                )
            }))
            .children(missing.map(|reference| {
                if form.kind == NativeKind::Package {
                    self.text("原主包不可用。请选择一个已安装主包。", 13.0, palette.danger)
                        .into_any_element()
                } else {
                    let short: String = reference.chars().take(12).collect();
                    self.configuration_row(
                        form.kind,
                        reference.clone(),
                        "已保存的资源不可用".into(),
                        short,
                        true,
                        cx,
                    )
                    .into_any_element()
                }
            }));
        let actions = div().flex().flex_wrap().justify_end().gap_2().children(
            (form.kind == NativeKind::Package)
                .then(|| self.resource_install_button(NativeKind::Package, cx))
                .flatten(),
        );
        setup.child(group).child(actions.when(
            self.can_save_resource_configuration(form, catalog),
            |body| {
                body.child(
                    self.configuration_button("save-project-resources", "保存项目配置")
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.save_resource_configuration(window, cx)
                        })),
                )
            },
        ))
    }

    fn configuration_row(
        &self,
        kind: NativeKind,
        reference: String,
        name: String,
        caption: String,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = self.palette;
        self.configuration_button(gpui::SharedString::from(reference.clone()), "")
            .justify_between()
            .gap_2()
            .h_auto()
            .min_h(px(44.0))
            .py_2()
            .when(selected, |row| row.bg(palette.subtle))
            .child(
                div()
                    .min_w_0()
                    .child(self.text(name, 13.0, palette.text))
                    .child(self.text(caption, 12.0, palette.muted)),
            )
            .child(
                self.text(
                    if selected {
                        if kind == NativeKind::Package {
                            "已选主包"
                        } else {
                            "移出配置"
                        }
                    } else {
                        "选择"
                    },
                    13.0,
                    palette.muted,
                )
                .flex_none(),
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(form) = shell.resource_settings.configuration_form.as_mut() {
                    form.choose(kind, &reference);
                    cx.notify();
                }
            }))
    }

    fn configuration_button(&self, id: impl Into<gpui::ElementId>, label: &str) -> Stateful<Div> {
        let palette = self.palette;
        self.button(id, label)
            .h(px(36.0))
            .tab_index(0)
            .border_1()
            .border_color(palette.line)
            .focus(|button| button.border_color(palette.strong_line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_save_stays_blocked_until_its_revision_is_overtaken() {
        let mut selection = empty_selection();
        selection.main_package = "project-package".into();
        let make_attempt = || Attempt {
            request: NativeApply {
                expected_revision: 3,
                operation_id: "fixture".into(),
                target: selection.clone(),
                supersedes_operation_id: None,
            },
            outcome: Outcome::Unconfirmed,
        };
        let mut attempt = make_attempt();
        let mut observed = NativeProjectResources {
            revision: 3,
            selection: Some(selection.clone()),
            diagnostic: None,
        };
        attempt.reconcile(&observed);
        assert!(attempt.outcome == Outcome::Unconfirmed);
        observed.revision = 4;
        attempt.reconcile(&observed);
        assert!(attempt.outcome == Outcome::Observed);
        let mut competing = make_attempt();
        observed
            .selection
            .as_mut()
            .unwrap()
            .skills
            .push("other-skill".into());
        competing.reconcile(&observed);
        assert!(competing.outcome == Outcome::Superseded);
    }
}
