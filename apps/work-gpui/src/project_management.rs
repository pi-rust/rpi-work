//! Register existing directories on the connected node.

use gpui::{Context, Div, Task, Window, div, prelude::*, px};
use work_client::{Project, ProjectFolders, WorkspaceClient};
use work_viewmodel::config_requests::bounded;

use super::{Overlay, Shell, editor::Buffer, rejected};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Field {
    #[default]
    Name,
    Path,
}

impl Field {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Name => "my-project",
            Self::Path => "目标节点上的已有目录路径",
        }
    }
}

#[derive(Default)]
pub(super) struct Form {
    pub(super) open: bool,
    field: Field,
    name: Buffer,
    path: Buffer,
    folders: Option<ProjectFolders>,
    browsing: Option<Task<()>>,
    registering: bool,
    follow_registration: bool,
    unverified: Option<Lookup>,
    error: Option<String>,
}

impl Form {
    fn field(&self, field: Field) -> &Buffer {
        match field {
            Field::Name => &self.name,
            Field::Path => &self.path,
        }
    }

    pub(super) fn buffer(&self) -> &Buffer {
        self.field(self.field)
    }

    pub(super) fn buffer_mut(&mut self) -> &mut Buffer {
        match self.field {
            Field::Name => &mut self.name,
            Field::Path => &mut self.path,
        }
    }

    pub(super) fn placeholder(&self) -> &'static str {
        self.field.placeholder()
    }

    fn editable(&self) -> bool {
        !self.registering && self.unverified.is_none()
    }

    fn valid(&self) -> bool {
        [(&self.name, 50), (&self.path, 300)]
            .into_iter()
            .all(|(field, limit)| {
                field.can_send()
                    && field.text.trim().chars().count() <= limit
                    && !field.text.chars().any(char::is_control)
            })
    }
}

pub(crate) enum Registration {
    Register { name: String, path: String },
    Verify(Lookup),
}

#[derive(Clone)]
pub(crate) struct Lookup {
    id: Option<String>,
    path: String,
}

pub(crate) enum ResultState {
    Registered(Project),
    Rejected(String),
    Unverified(Lookup, String),
}

impl Registration {
    pub(crate) async fn run(self, client: &WorkspaceClient) -> ResultState {
        let lookup = match self {
            Self::Register { name, path } => {
                match bounded("添加项目", client.create_project(&name, Some(&path), false)).await
                {
                    Ok(project) if !project.id.is_empty() && !project.path.is_empty() => Lookup {
                        id: Some(project.id),
                        path: project.path,
                    },
                    Err(error) if rejected(&error) => {
                        return ResultState::Rejected(error.to_string());
                    }
                    _ => Lookup { id: None, path },
                }
            }
            Self::Verify(lookup) => lookup,
        };
        let canonical = if lookup.id.is_none() {
            bounded("核对目录", client.project_folders(&lookup.path))
                .await
                .ok()
                .map(|listing| listing.path)
        } else {
            None
        };
        match bounded("核对项目列表", client.projects()).await {
            Ok(projects) => projects
                .into_iter()
                .find(|project| match lookup.id.as_ref() {
                    Some(id) => project.id == *id && project.path == lookup.path,
                    None => {
                        project.path == lookup.path
                            || Some(project.path.as_str()) == canonical.as_deref()
                    }
                })
                .map_or_else(
                    || ResultState::Unverified(lookup, "项目尚未出现在节点列表中。".into()),
                    ResultState::Registered,
                ),
            Err(error) => ResultState::Unverified(lookup, error.to_string()),
        }
    }
}

impl Shell {
    pub(super) fn dismiss_project_form(&mut self) -> bool {
        if !self.project_form.open {
            return false;
        }
        self.project_form.open = false;
        self.project_form.follow_registration = false;
        self.project_form.browsing = None;
        self.input_layout = None;
        self.input_dragging = false;
        true
    }

    pub(super) fn next_project_field(&mut self, cx: &mut Context<Self>) {
        if self.project_form.editable() {
            let field = if self.project_form.field == Field::Name {
                Field::Path
            } else {
                Field::Name
            };
            self.select_project_field(field, cx);
        }
    }

    fn select_project_field(&mut self, field: Field, cx: &mut Context<Self>) {
        self.project_form.buffer_mut().finish_composition();
        self.project_form.field = field;
        self.input_layout = None;
        self.input_dragging = false;
        cx.notify();
    }

    fn browse_project_folders(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(node) = self.node.as_ref() else {
            return;
        };
        self.project_form.error = None;
        let request = node.project_folders(path);
        self.project_form.browsing = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |shell, cx| {
                let form = &mut shell.project_form;
                form.browsing = None;
                match result {
                    Ok(Ok(listing)) => form.folders = Some(listing),
                    Ok(Err(error)) => form.error = Some(error.to_string()),
                    Err(error) => form.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn submit_project_form(&mut self, cx: &mut Context<Self>) {
        let Some(node) = self.node.as_ref() else {
            return;
        };
        let form = &mut self.project_form;
        if !form.open || form.registering || form.browsing.is_some() {
            return;
        }
        let request = if let Some(lookup) = form.unverified.clone() {
            Registration::Verify(lookup)
        } else if form.valid() {
            Registration::Register {
                name: form.name.text.trim().into(),
                path: form.path.text.trim().into(),
            }
        } else {
            form.error = Some("请填写名称（最多 50 字）和已有目录路径（最多 300 字）。".into());
            cx.notify();
            return;
        };
        form.registering = true;
        form.follow_registration = true;
        form.error = None;
        form.folders = None;
        let navigation = self.navigation_revision;
        let work = node.register_project(request);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                shell.project_form.registering = false;
                match result {
                    Ok(ResultState::Registered(project)) => {
                        let index = shell
                            .snapshot
                            .projects
                            .iter()
                            .position(|item| item.id == project.id)
                            .unwrap_or(shell.snapshot.projects.len());
                        let follow = shell.project_form.follow_registration;
                        shell.dismiss_project_form();
                        shell.project_form = Form::default();
                        if let Some(item) = shell.snapshot.projects.get_mut(index) {
                            item.name = project.name;
                        } else {
                            shell.snapshot.projects.push(project.into());
                        }
                        if follow && shell.navigation_revision == navigation {
                            shell.select_project(index, cx);
                            if shell.overlay == Some(Overlay::Navigation) {
                                shell.overlay = None;
                            }
                        }
                    }
                    Ok(ResultState::Rejected(message)) => shell.project_form.error = Some(message),
                    Ok(ResultState::Unverified(lookup, message)) => {
                        shell.project_form.unverified = Some(lookup);
                        shell.project_form.error = Some(message);
                    }
                    Err(error) => {
                        let form = &mut shell.project_form;
                        form.unverified.get_or_insert_with(|| Lookup {
                            id: None,
                            path: form.path.text.trim().into(),
                        });
                        form.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn add_project_button(&self, cx: &Context<Self>) -> impl IntoElement {
        self.icon_button("add-project", "plus", 24.0)
            .on_click(cx.listener(|shell, _, window: &mut Window, cx| {
                shell.cancel_comment();
                if let Some(draft) = shell.drafts.get_mut(&shell.session) {
                    draft.input.finish_composition();
                }
                shell.project_form.open = true;
                shell.focus_composer(window);
                shell.input_layout = None;
                cx.notify();
            }))
    }

    pub(super) fn render_project_form(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let form = &self.project_form;
        let mut fields = Vec::new();
        for (field, label) in [(Field::Name, "名称"), (Field::Path, "项目路径")] {
            let buffer = form.field(field);
            let active = form.editable() && form.field == field;
            let value = if buffer.text.is_empty() {
                field.placeholder()
            } else {
                &buffer.text
            };
            let field_box = div()
                .id(label)
                .flex_1()
                .min_w(px(0.0))
                .border_1()
                .rounded(px(6.0))
                .border_color(palette.strong_line)
                .when(active, |field_box| field_box.child(self.input_element(cx)))
                .when(!active, |field_box| {
                    field_box
                        .cursor(gpui::CursorStyle::IBeam)
                        .px_2()
                        .py_1()
                        .child(
                            self.text(
                                value.to_owned(),
                                13.0,
                                if buffer.text.is_empty() {
                                    palette.muted
                                } else {
                                    palette.text
                                },
                            )
                            .truncate(),
                        )
                        .when(form.editable(), |field_box| {
                            field_box.on_click(cx.listener(move |shell, _, window, cx| {
                                shell.select_project_field(field, cx);
                                shell.focus_composer(window);
                            }))
                        })
                });
            fields.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text(label, 13.0, palette.text))
                    .child(div().flex().items_center().gap_2().child(field_box).when(
                        field == Field::Path && form.editable(),
                        |row| {
                            let path = buffer.text.trim().to_owned();
                            row.child(
                                self.button("browse-project-folders", "选择文件夹")
                                    .flex_none()
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        shell.browse_project_folders(path.clone(), cx)
                                    })),
                            )
                        },
                    )),
            );
        }
        let close = cx.listener(|shell, _, _, cx| {
            shell.dismiss_project_form();
            cx.notify();
        });
        let label = if form.registering {
            "正在核对…"
        } else if form.unverified.is_some() {
            "核对项目列表"
        } else {
            "添加项目"
        };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000039))
            .flex()
            .items_center()
            .justify_center()
            .px_4()
            .child(
                div()
                    .id("project-form")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .max_w(px(450.0))
                    .max_h_full()
                    .overflow_y_scroll()
                    .p_4()
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .rounded(px(12.0))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(self.text("添加项目", 16.0, palette.text))
                            .child(
                                self.icon_button("close-add-project", "x", 24.0)
                                    .on_click(close),
                            ),
                    )
                    .child(self.text(
                        "将已有项目加入工作台，添加后即可开始对话。",
                        12.0,
                        palette.muted,
                    ))
                    .children(fields.drain(..1))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(self.text("所属节点", 13.0, palette.text))
                            .child(
                                self.text(self.snapshot.node.clone(), 12.0, palette.muted)
                                    .truncate()
                                    .px_2()
                                    .py_1(),
                            ),
                    )
                    .children(fields)
                    .when(form.browsing.is_some(), |dialog| {
                        dialog.child(self.text("正在读取节点目录…", 12.0, palette.muted))
                    })
                    .when_some(form.folders.as_ref(), |dialog, folders| {
                        let path = folders.path.clone();
                        dialog
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        self.text(path.clone(), 12.0, palette.muted)
                                            .truncate()
                                            .flex_1(),
                                    )
                                    .child(
                                        self.button("use-project-folder", "使用此目录").on_click(
                                            cx.listener(move |shell, _, _, cx| {
                                                shell.project_form.path = Buffer::default();
                                                shell.project_form.path.replace(None, &path);
                                                shell.project_form.folders = None;
                                                shell.select_project_field(Field::Path, cx);
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("project-folders")
                                    .flex()
                                    .flex_col()
                                    .max_h(px(135.0))
                                    .overflow_y_scroll()
                                    .children(folders.folders.iter().enumerate().map(
                                        |(index, folder)| {
                                            let path = folder.path.clone();
                                            self.button("open-project-folder", &folder.name)
                                                .id(("open-project-folder", index))
                                                .justify_start()
                                                .on_click(cx.listener(move |shell, _, _, cx| {
                                                    shell.browse_project_folders(path.clone(), cx)
                                                }))
                                        },
                                    )),
                            )
                    })
                    .when(form.unverified.is_some(), |dialog| {
                        dialog.child(self.text(
                            "添加结果待确认。可重新核对节点列表。",
                            12.0,
                            palette.muted,
                        ))
                    })
                    .when_some(form.error.as_ref(), |dialog, error| {
                        dialog.child(self.text(error.clone(), 12.0, palette.danger))
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                self.button("cancel-add-project", "取消")
                                    .on_click(cx.listener(|shell, _, _, cx| {
                                        shell.dismiss_project_form();
                                        cx.notify();
                                    })),
                            )
                            .child(self.button("submit-add-project", label).when(
                                !form.registering && form.browsing.is_none(),
                                |button| {
                                    button.bg(palette.subtle).on_click(
                                        cx.listener(|shell, _, _, cx| {
                                            shell.submit_project_form(cx)
                                        }),
                                    )
                                },
                            )),
                    ),
            )
    }
}
