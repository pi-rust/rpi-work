//! Read-only project directory, source and change views.

use gpui::{
    AnyElement, ClipboardItem, Context, Div, HighlightStyle, IntoElement, SharedString, StyledText,
    Task, div, prelude::*, px,
};
use work_client::{ClientError, Diff, FileContent, FileListing, GitStatus, WorkspaceClient};
use work_viewmodel::{diff_view, file_view};

use super::Shell;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Overview,
    Files,
    Changes,
}

impl Tab {
    fn root(self) -> Option<Route> {
        match self {
            Self::Overview => None,
            Self::Files => Some(Route::Directory {
                path: String::new(),
                cursor: None,
            }),
            Self::Changes => Some(Route::Changes),
        }
    }
}

#[derive(Clone)]
pub(crate) enum Route {
    Directory {
        path: String,
        cursor: Option<String>,
    },
    Source(String),
    Changes,
    Diff {
        path: String,
        staged: bool,
    },
}

impl Route {
    pub(crate) async fn read(
        self,
        client: &WorkspaceClient,
        id: &str,
    ) -> Result<Contents, ClientError> {
        match self {
            Self::Directory { path, cursor } => client
                .files_page(id, &path, cursor.as_deref())
                .await
                .map(Contents::Directory),
            Self::Source(path) => {
                let source = client.file(id, &path).await?;
                if source.path != path {
                    return Err(ClientError::Protocol(
                        "file path does not match request".into(),
                    ));
                }
                Ok(Contents::Source(source))
            }
            Self::Changes => client.git_status(id).await.map(Contents::Changes),
            Self::Diff { path, staged } => {
                let diff = client.diff(id, &path, staged).await?;
                if diff.path != path || diff.staged.is_some_and(|returned| returned != staged) {
                    return Err(ClientError::Protocol("diff does not match request".into()));
                }
                Ok(Contents::Diff(diff))
            }
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Directory { path, .. } if path.is_empty() => "项目根目录",
            Self::Directory { path, .. } | Self::Source(path) | Self::Diff { path, .. } => path,
            Self::Changes => "工作树变更",
        }
    }
}

pub(crate) enum Contents {
    Directory(FileListing),
    Source(FileContent),
    Changes(GitStatus),
    Diff(Diff),
}

struct Pending {
    _display: Task<()>,
    request: tokio::task::AbortHandle,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.request.abort();
    }
}

#[derive(Default)]
pub(super) struct Panel {
    tab: Tab,
    route: Option<Route>,
    history: Vec<Route>,
    sequence: u64,
    pending: Option<Pending>,
    result: Option<Result<Contents, String>>,
}

impl Panel {
    pub(super) fn is_overview(&self) -> bool {
        self.tab == Tab::Overview
    }
}

impl Shell {
    pub(super) fn reset_project_files(&mut self, cx: &mut Context<Self>) {
        self.files.history.clear();
        self.read_project(self.files.tab.root(), cx);
    }

    fn open_project_path(&mut self, route: Route, cx: &mut Context<Self>) {
        self.files.history.extend(self.files.route.take());
        self.read_project(Some(route), cx);
    }

    fn read_project(&mut self, route: Option<Route>, cx: &mut Context<Self>) {
        self.files.pending = None;
        self.files.result = None;
        self.files.route = route.clone();
        self.files.sequence += 1;
        cx.notify();
        let Some(route) = route else { return };
        let Some(project) = self
            .project
            .and_then(|index| self.snapshot.projects.get(index))
        else {
            self.files.result = Some(Err("请先选择项目。".into()));
            return;
        };
        let Some(node) = self.node.as_ref() else {
            self.files.result = Some(Err("连接节点后可读取项目文件。".into()));
            return;
        };
        let work = node.read_project(project.id.clone(), route);
        let request = work.abort_handle();
        let sequence = self.files.sequence;
        let display = cx.spawn(async move |this, cx| {
            let result = match work.await {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update(cx, |shell, cx| {
                if shell.files.sequence == sequence {
                    shell.files.result = Some(result);
                    cx.notify();
                }
            });
        });
        self.files.pending = Some(Pending {
            _display: display,
            request,
        });
    }

    pub(super) fn project_tabs(&self, cx: &mut Context<Self>) -> Div {
        div().flex().items_center().gap_1().children(
            [
                (Tab::Overview, "project-overview", "项目概览"),
                (Tab::Files, "project-files", "文件"),
                (Tab::Changes, "project-changes", "变更"),
            ]
            .into_iter()
            .map(|(tab, id, label)| {
                self.button(id, label)
                    .when(self.files.tab == tab, |button| {
                        button.bg(self.palette.hover)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.files.tab != tab {
                            this.files.tab = tab;
                            this.reset_project_files(cx);
                        }
                    }))
            }),
        )
    }

    pub(super) fn project_contents(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(route) = self.files.route.as_ref() else {
            return div().into_any_element();
        };
        let toolbar = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(self.palette.line)
            .when(!self.files.history.is_empty(), |row| {
                row.child(self.button("project-back", "返回").on_click(cx.listener(
                    |this, _, _, cx| {
                        let previous = this.files.history.pop();
                        this.read_project(previous, cx);
                    },
                )))
            })
            .child(
                self.text(route.title().to_owned(), 13.0, self.palette.text)
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate(),
            )
            .when(
                self.files.result.is_some() && self.node.is_some() && self.project.is_some(),
                |row| {
                    row.child(self.button("project-refresh", "刷新").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.read_project(this.files.route.clone(), cx);
                        },
                    )))
                },
            );
        let content = match &self.files.result {
            None => self.project_note("正在读取…", false),
            Some(Err(error)) => self.project_note(error, true),
            Some(Ok(Contents::Directory(listing))) => {
                self.directory(listing, cx).into_any_element()
            }
            Some(Ok(Contents::Changes(status))) => self.changes(status, cx),
            Some(Ok(Contents::Source(file))) => self.file_preview(&file.path, &file.content, None),
            Some(Ok(Contents::Diff(diff))) => {
                let staged = matches!(route, Route::Diff { staged: true, .. });
                self.file_preview(&diff.path, &diff.patch, Some(staged))
            }
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(toolbar)
            .child(
                div()
                    .id(("project-content", self.files.sequence))
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .child(content),
            )
            .into_any_element()
    }

    fn project_note(&self, message: &str, error: bool) -> AnyElement {
        let color = if error {
            self.palette.danger
        } else {
            self.palette.muted
        };
        self.text(message.to_owned(), 13.0, color)
            .p_4()
            .into_any_element()
    }

    fn directory(&self, listing: &FileListing, cx: &mut Context<Self>) -> Div {
        div()
            .p_2()
            .when(listing.entries.is_empty(), |body| {
                body.child(self.project_note("目录为空。", false))
            })
            .children(listing.entries.iter().enumerate().map(|(index, entry)| {
                let target = if entry.kind == "directory" {
                    Route::Directory {
                        path: entry.path.clone(),
                        cursor: None,
                    }
                } else {
                    Route::Source(entry.path.clone())
                };
                let title = if entry.kind == "directory" {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                div()
                    .id(("file", index))
                    .rounded(px(6.0))
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .hover(|row| row.bg(self.palette.hover))
                    .child(self.text(title, 13.0, self.palette.text).truncate())
                    .on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.open_project_path(target.clone(), cx)
                        }),
                    )
            }))
            .when_some(
                listing
                    .pagination
                    .as_ref()
                    .and_then(|page| page.next_cursor.as_ref()),
                |body, cursor| {
                    let next = Route::Directory {
                        path: listing.path.clone(),
                        cursor: Some(cursor.clone()),
                    };
                    body.child(
                        self.button("directory-next-page", "下一页")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_project_path(next.clone(), cx);
                            })),
                    )
                },
            )
    }

    fn changes(&self, status: &GitStatus, cx: &mut Context<Self>) -> AnyElement {
        if !status.is_repository {
            return self.project_note("该项目没有 Git 工作树。", false);
        }
        if status.files.is_empty() {
            return self.project_note("工作树没有改动。", false);
        }
        div()
            .flex()
            .flex_col()
            .p_2()
            .children(status.files.iter().enumerate().map(|(index, file)| {
                let path = file.path.clone();
                let untracked = file.status == "??";
                div()
                    .id(("change", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(self.text(file.status.clone(), 12.0, self.palette.muted))
                    .child(
                        self.text(path, 13.0, self.palette.text)
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate(),
                    )
                    .children(
                        [
                            (true, file.staged, "staged", "已暂存"),
                            (false, file.unstaged, "unstaged", "未暂存"),
                        ]
                        .into_iter()
                        .filter(|&(_, present, _, _)| present && !untracked)
                        .map(|(staged, _, id, label)| {
                            let target = Route::Diff {
                                path: file.path.clone(),
                                staged,
                            };
                            self.button(id, label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_project_path(target.clone(), cx)
                                }))
                        }),
                    )
                    .when(untracked, |row| {
                        let path = file.path.clone();
                        row.child(
                            self.button("untracked-source", "源码")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_project_path(Route::Source(path.clone()), cx);
                                })),
                        )
                    })
            }))
            .into_any_element()
    }

    fn file_preview(&self, path: &str, source: &str, staged: Option<bool>) -> AnyElement {
        let copy = SharedString::from(source.to_owned());
        let label = match staged {
            Some(true) => "已暂存差异",
            Some(false) => "未暂存差异",
            None => "源码",
        };
        let mut body = div()
            .flex()
            .flex_col()
            .font_family("Menlo")
            .text_size(px(12.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .text_color(self.palette.text);
        if let Some(_staged) = staged {
            let diff = diff_view::present(source, path);
            body = body.children(diff.rows.iter().map(|row| {
                self.file_line(
                    &row.line,
                    diff.structured.then(|| {
                        format!(
                            "{:>width$} {:>width$}",
                            row.old_number.map(|n| n.to_string()).unwrap_or_default(),
                            row.new_number.map(|n| n.to_string()).unwrap_or_default(),
                            width = diff.gutter_digits,
                        )
                    }),
                )
            }));
        } else {
            let lines = file_view::present(source, path, false);
            let width = lines.len().to_string().len();
            body = body.children(lines.iter().map(|line| {
                self.file_line(line, line.number.map(|number| format!("{number:>width$}")))
            }));
        }
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1()
                    .bg(self.palette.subtle)
                    .child(self.text(label, 12.0, self.palette.muted))
                    .child(
                        self.button("copy-project-text", "复制")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()));
                            }),
                    ),
            )
            .child(if source.is_empty() {
                self.project_note("内容为空。", false)
            } else {
                body.into_any_element()
            })
            .into_any_element()
    }

    fn file_line(&self, line: &file_view::FileLine<'_>, gutter: Option<String>) -> Div {
        let mut text = String::new();
        let mut highlights = Vec::new();
        for token in &line.tokens {
            let start = text.len();
            text.push_str(token.text);
            let color = match line.kind {
                "add" => self.palette.success,
                "remove" => self.palette.danger,
                "meta" => self.palette.muted,
                _ => self.palette.code_token(token.class),
            };
            if text.len() > start {
                highlights.push((
                    start..text.len(),
                    HighlightStyle {
                        color: Some(color),
                        ..Default::default()
                    },
                ));
            }
        }
        div()
            .flex()
            .px_3()
            .gap_3()
            .min_h(px(20.0))
            .when_some(gutter, |row, gutter| {
                row.child(
                    div()
                        .flex_none()
                        .text_color(self.palette.muted)
                        .child(gutter),
                )
            })
            .child(
                div()
                    .flex_none()
                    .child(StyledText::new(text).with_highlights(highlights)),
            )
    }
}
