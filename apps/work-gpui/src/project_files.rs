//! Read-only project directory, source and change views.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, ClipboardItem, Context, Div, HighlightStyle, IntoElement, SharedString, StyledText,
    Task, div, prelude::*, px,
};
use work_client::{ClientError, Diff, FileContent, FileListing, GitStatus, WorkspaceClient};
use work_core::api::ChangedFile;
use work_viewmodel::diff_view::LineCounts;
use work_viewmodel::{diff_view, file_view};

use super::Shell;
use crate::icons::icon;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Overview,
    Files,
    Changes,
    #[cfg(target_os = "macos")]
    Browser(u64),
}

impl Tab {
    pub(super) fn browser_id(self) -> Option<u64> {
        #[cfg(target_os = "macos")]
        if let Self::Browser(id) = self {
            return Some(id);
        }
        None
    }

    fn root(self) -> Option<Route> {
        match self {
            Self::Overview => None,
            Self::Files => Some(Route::Directory {
                path: String::new(),
                cursor: None,
            }),
            Self::Changes => Some(Route::Changes),
            #[cfg(target_os = "macos")]
            Self::Browser(_) => None,
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

pub(crate) async fn read_line_counts(
    client: &WorkspaceClient,
    project: &str,
    files: Vec<ChangedFile>,
    output: tokio::sync::mpsc::Sender<(String, LineCounts)>,
) {
    let mut remaining = files.into_iter();
    let mut pending = tokio::task::JoinSet::new();
    loop {
        // Keep the preview responsive without sending a request for every file at once.
        for file in remaining.by_ref().take(3 - pending.len()) {
            let client = client.clone();
            let project = project.to_owned();
            pending.spawn(async move {
                let mut counts = LineCounts::default();
                for staged in [true, false]
                    .into_iter()
                    .filter(|scope| if *scope { file.staged } else { file.unstaged })
                {
                    let route = Route::Diff {
                        path: file.path.clone(),
                        staged,
                    };
                    let Ok(Contents::Diff(diff)) = work_viewmodel::config_requests::bounded(
                        "读取 Git 增删统计",
                        route.read(&client, &project),
                    )
                    .await
                    else {
                        return None;
                    };
                    counts = counts + diff_view::present(&diff.patch, &diff.path).line_counts()?;
                }
                (file.staged || file.unstaged).then_some((file.path, counts))
            });
        }
        match pending.join_next().await {
            Some(Ok(Some(counts))) => {
                if output.send(counts).await.is_err() {
                    break;
                }
            }
            Some(_) => {}
            None => break,
        }
    }
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
    pub(super) tab: Tab,
    route: Option<Route>,
    history: Vec<Route>,
    sequence: u64,
    pending: Option<Pending>,
    result: Option<Result<Contents, String>>,
    changes: Option<GitStatus>,
    line_counts: HashMap<String, LineCounts>,
    counts_pending: Option<Pending>,
    review_check: Option<Pending>,
    review_error: Option<String>,
}

impl Panel {
    pub(super) fn line_totals(&self) -> Option<LineCounts> {
        let files = &self.changes.as_ref()?.files;
        if files.is_empty() || files.len() != self.line_counts.len() {
            return None;
        }
        Some(
            self.line_counts
                .values()
                .copied()
                .fold(LineCounts::default(), |sum, counts| sum + counts),
        )
    }

    pub(super) fn is_overview(&self) -> bool {
        self.tab == Tab::Overview
    }

    pub(super) fn cancel_review_check(&mut self) {
        self.review_check = None;
        self.review_error = None;
    }

    pub(super) fn set_review_notice(&mut self, message: impl Into<String>) {
        self.review_error = Some(message.into());
    }

    pub(super) fn clear_review_notice(&mut self) {
        self.review_error = None;
    }

    pub(super) fn checking_review(&self) -> bool {
        self.review_check.is_some()
    }
}

impl Shell {
    pub(super) fn reset_project_files(&mut self, cx: &mut Context<Self>) {
        self.files.cancel_review_check();
        self.files.history.clear();
        self.files.changes = None;
        self.files.line_counts.clear();
        self.files.counts_pending = None;
        self.read_project(self.files.tab.root(), cx);
    }

    fn observe_line_counts(&mut self, cx: &mut Context<Self>) {
        self.files.line_counts.clear();
        self.files.counts_pending = None;
        let (Some(node), Some(project), Some(changes)) = (
            self.node.as_ref(),
            self.project,
            self.files.changes.as_ref(),
        ) else {
            return;
        };
        if !changes.is_repository || changes.files.is_empty() {
            return;
        }
        let (mut updates, request) = node.project_line_counts(
            self.snapshot.projects[project].id.clone(),
            changes.files.clone(),
        );
        let display = cx.spawn(async move |this, cx| {
            while let Some((path, counts)) = updates.recv().await {
                if this
                    .update(cx, |shell, cx| {
                        shell.files.line_counts.insert(path, counts);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.files.counts_pending = Some(Pending {
            _display: display,
            request,
        });
    }

    pub(super) fn git_line_counts(&self, counts: LineCounts) -> Div {
        div()
            .flex()
            .flex_none()
            .gap(px(6.0))
            .font_family("Menlo")
            .child(self.text(format!("+{}", counts.added), 11.0, self.palette.success))
            .child(self.text(format!("−{}", counts.removed), 11.0, self.palette.danger))
    }

    fn open_project_path(&mut self, route: Route, cx: &mut Context<Self>) {
        self.files.history.extend(self.files.route.take());
        self.read_project(Some(route), cx);
    }

    fn read_project(&mut self, route: Option<Route>, cx: &mut Context<Self>) {
        self.cancel_comment();
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
                    if let Ok(Contents::Changes(status)) = result {
                        let first = status.files.iter().find_map(|file| {
                            if file.status == "??" {
                                Some(Route::Source(file.path.clone()))
                            } else if file.unstaged || file.staged {
                                Some(Route::Diff {
                                    path: file.path.clone(),
                                    staged: !file.unstaged,
                                })
                            } else {
                                None
                            }
                        });
                        shell.files.changes = Some(status);
                        if first.is_some() {
                            shell.read_project(first, cx);
                        } else {
                            cx.notify();
                        }
                        shell.observe_line_counts(cx);
                    } else {
                        shell.files.result = Some(result);
                        cx.notify();
                    }
                }
            });
        });
        self.files.pending = Some(Pending {
            _display: display,
            request,
        });
    }

    pub(super) fn show_project_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.files.tab != tab {
            self.files.tab = tab;
            self.reset_project_files(cx);
        }
    }

    pub(super) fn project_tabs(&self, cx: &mut Context<Self>) -> Div {
        div().flex().items_center().gap_1().children(
            [
                (Tab::Overview, "project-overview", "globe", "项目概览"),
                (Tab::Files, "project-files", "code-xml", "文件"),
                (Tab::Changes, "project-changes", "file-diff", "变更"),
            ]
            .into_iter()
            .map(|(tab, id, shape, label)| {
                self.button(id, "")
                    .gap(px(6.0))
                    .child(crate::icons::icon(shape, 15.0, self.palette.muted))
                    .child(self.text(label, 12.0, self.palette.muted))
                    .when(self.files.tab == tab, |button| {
                        button.bg(self.palette.subtle)
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
        if self.files.tab == Tab::Changes {
            return self.review_panel(cx);
        }
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
            Some(Ok(Contents::Changes(_))) => unreachable!("Git status is stored separately"),
            Some(Ok(Contents::Source(file))) => self.file_preview(&file.path, &file.content),
            Some(Ok(Contents::Diff(_))) => unreachable!("diffs belong to the review tab"),
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

    fn can_run_review(&self) -> bool {
        self.session_controls().send
            && self.review_after_bind.is_none()
            && (!self.preferences.detached_review
                || (self.can_create_session() && self.creation_readback.is_none()))
    }

    fn run_review(&mut self, cx: &mut Context<Self>) {
        let Some((node, project)) = self.node.as_ref().zip(self.project) else {
            return;
        };
        if !self.can_run_review() {
            return;
        }
        let detached = self.preferences.detached_review;
        self.files.review_error = None;
        let work = node.read_project(self.snapshot.projects[project].id.clone(), Route::Changes);
        let request = work.abort_handle();
        let display = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                shell.files.review_check = None;
                match result {
                    Ok(Ok(Contents::Changes(status)))
                        if status.is_repository && !status.files.is_empty() =>
                    {
                        if shell.session_controls().send {
                            if detached {
                                if shell.can_create_session() && shell.creation_readback.is_none() {
                                    shell.files.set_review_notice("正在创建独立审阅会话…");
                                    shell.create_session(super::SessionCreation::Review, cx);
                                } else {
                                    shell
                                        .files
                                        .set_review_notice("请先核对现有的会话创建请求。");
                                }
                            } else {
                                shell.send_prompt(true, cx);
                            }
                        } else {
                            shell.files.review_error = Some("会话状态已变化，审阅未提交。".into());
                        }
                    }
                    Ok(Ok(Contents::Changes(status))) => {
                        shell.read_project(None, cx);
                        shell.files.changes = Some(status);
                        shell.observe_line_counts(cx);
                    }
                    _ => shell.files.review_error = Some("无法读取工作树，请重试。".into()),
                }
                cx.notify();
            });
        });
        self.files.review_check = Some(Pending {
            _display: display,
            request,
        });
        cx.notify();
    }

    fn review_panel(&self, cx: &Context<Self>) -> AnyElement {
        let palette = self.palette;
        let Some(status) = self.files.changes.as_ref() else {
            let message = match self.files.result.as_ref() {
                Some(Err(error)) => self.project_note(error, true),
                _ => self.project_note("正在读取工作树…", false),
            };
            return div()
                .child(message)
                .when(
                    self.files.result.is_some() && self.node.is_some(),
                    |panel| {
                        panel.child(
                            self.button("review-retry", "重新读取")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reset_project_files(cx);
                                })),
                        )
                    },
                )
                .into_any_element();
        };
        if !status.is_repository {
            return self.project_note("该项目没有 Git 工作树。", false);
        }
        let summary = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .h(px(39.0))
            .border_b_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .min_w(px(0.0))
                    .child(icon("git-branch", 13.0, palette.muted))
                    .child(
                        self.text(
                            status
                                .branch
                                .clone()
                                .unwrap_or_else(|| "分离的 HEAD".into()),
                            11.0,
                            palette.muted,
                        )
                        .font_family("Menlo")
                        .truncate(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_none()
                    .child(self.text(
                        format!("{} 个文件已修改", status.files.len()),
                        11.0,
                        palette.muted,
                    ))
                    .when_some(self.files.line_totals(), |summary, counts| {
                        summary.child(self.git_line_counts(counts))
                    }),
            );
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .mt(px(8.0))
            .mb(px(4.0))
            .child(self.text("变更文件", 12.0, palette.text))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.button("refresh-changes", "刷新").h(px(24.0)).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.reset_project_files(cx);
                        }),
                    ))
                    .when(!status.files.is_empty(), |header| {
                        let enabled = self.can_run_review();
                        header
                            .child(
                                self.button(
                                    "review-delivery",
                                    if self.preferences.detached_review {
                                        "独立会话"
                                    } else {
                                        "当前会话"
                                    },
                                )
                                .h(px(24.0))
                                .when(
                                    !self.files.checking_review()
                                        && self.creating_session.is_none()
                                        && self.review_after_bind.is_none(),
                                    |button| {
                                        button.on_click(cx.listener(|this, _, window, cx| {
                                            this.preferences.detached_review =
                                                !this.preferences.detached_review;
                                            this.save_preferences(window, cx);
                                        }))
                                    },
                                ),
                            )
                            .child(
                                self.button(
                                    "run-review",
                                    if self.files.checking_review() {
                                        "正在核对工作树…"
                                    } else {
                                        "运行审阅"
                                    },
                                )
                                .h(px(24.0))
                                .when(enabled, |button| {
                                    button.on_click(cx.listener(|this, _, _, cx| {
                                        this.run_review(cx);
                                    }))
                                })
                                .when(!enabled, |button| button.opacity(0.6)),
                            )
                    }),
            );
        let files = status.files.iter().enumerate().filter_map(|(index, file)| {
            let unstaged = file.unstaged;
            let staged = file.staged;
            let path = file.path.clone();
            let target = if file.status == "??" {
                Route::Source(path.clone())
            } else if staged || unstaged {
                Route::Diff { path: path.clone(), staged: !unstaged }
            } else { return None };
            let selected = matches!(self.files.route.as_ref(), Some(Route::Diff { path: active, .. } | Route::Source(active)) if active == &path);
            Some(div().id(("change", index)).flex().items_center().gap(px(7.0)).h(px(34.0)).px(px(7.0))
                .rounded(px(7.0)).cursor_pointer().hover(|style| style.bg(palette.hover))
                .when(selected, |row| row.bg(palette.subtle))
                .child(icon("file-diff", 14.0, palette.muted))
                .child(self.text(path, 12.0, palette.text).font_family("Menlo").flex_1().min_w(px(0.0)).truncate())
                .when_some(self.files.line_counts.get(&file.path), |row, counts| row.child(self.git_line_counts(*counts)))
                .when(!self.files.line_counts.contains_key(&file.path) || staged, |row| {
                    row.child(self.text(if staged { "已暂存".to_owned() } else { file.status.clone() }, 11.0, palette.muted).flex_none())
                })
                .on_click(cx.listener(move |this, _, _, cx| this.read_project(Some(target.clone()), cx))))
        });
        let preview = match (&self.files.route, &self.files.result) {
            (Some(Route::Diff { path, staged }), result) => {
                let file = status.files.iter().find(|file| &file.path == path);
                let patch = match result {
                    Some(Ok(Contents::Diff(diff))) => Some(diff.patch.as_str()),
                    _ => None,
                };
                let caption = div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .h(px(38.0))
                    .px(px(9.0))
                    .bg(palette.subtle)
                    .child(
                        self.text(path.clone(), 12.0, palette.text)
                            .font_family("Menlo")
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate(),
                    )
                    .children(
                        [
                            ("staged-preview", true, "已暂存"),
                            ("unstaged-preview", false, "未暂存"),
                        ]
                        .into_iter()
                        .filter(|&(_, scope, _)| {
                            file.is_some_and(|file| if scope { file.staged } else { file.unstaged })
                        })
                        .map(|(id, scope, label)| {
                            let path = path.clone();
                            self.button(id, label)
                                .h(px(23.0))
                                .px(px(5.0))
                                .when(*staged == scope, |button| {
                                    button.bg(palette.bg).border_1().border_color(palette.line)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.read_project(
                                        Some(Route::Diff {
                                            path: path.clone(),
                                            staged: scope,
                                        }),
                                        cx,
                                    );
                                }))
                        }),
                    )
                    .when_some(patch, |bar, patch| {
                        let copy = SharedString::from(patch.to_owned());
                        bar.child(
                            self.icon_button("copy-patch", "copy", 23.0)
                                .h(px(23.0))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy.to_string(),
                                    ))
                                }),
                        )
                    });
                div()
                    .mt(px(15.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(palette.line)
                    .overflow_hidden()
                    .child(caption)
                    .child(match result {
                        Some(Err(error)) => self.project_note(error, true),
                        Some(Ok(Contents::Diff(diff))) => self.diff_preview(diff, *staged, cx),
                        _ => self.project_note("正在读取差异…", false),
                    })
                    .into_any_element()
            }
            (Some(Route::Source(_)), Some(Ok(Contents::Source(file)))) => div()
                .mt(px(15.0))
                .border_1()
                .border_color(palette.line)
                .rounded(px(7.0))
                .overflow_hidden()
                .child(self.file_preview(&file.path, &file.content))
                .into_any_element(),
            (_, Some(Err(error))) => self.project_note(error, true),
            (Some(Route::Source(_)), None) => self.project_note("正在读取源码…", false),
            _ => div().into_any_element(),
        };
        div()
            .id("project-review")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(15.0))
                    .pt(px(8.0))
                    .pb(px(24.0))
                    .child(summary)
                    .child(header)
                    .when_some(self.files.review_error.as_deref(), |body, error| {
                        body.child(self.project_note(error, true))
                    })
                    .when(status.files.is_empty(), |body| {
                        body.child(self.project_note("工作树没有改动。", false))
                    })
                    .children(files)
                    .child(preview),
            )
            .into_any_element()
    }

    fn diff_preview(&self, diff: &Diff, staged: bool, cx: &Context<Self>) -> AnyElement {
        if diff.patch.is_empty() {
            return self.project_note("此范围没有文本差异。", false);
        }
        let palette = self.palette;
        let view = diff_view::present(&diff.patch, &diff.path);
        let observed = (self.frame.is_some() && view.feedback_revision.is_some())
            .then(|| Rc::new(diff.clone()));
        let feedback = self
            .drafts
            .get(&self.draft_key())
            .into_iter()
            .flat_map(|draft| &draft.feedback)
            .filter(|item| {
                item.diff.as_ref().is_some_and(|anchor| {
                    Some(&anchor.revision) == view.feedback_revision.as_ref()
                        && anchor.path == diff.path
                        && anchor.staged == staged
                })
            })
            .collect::<Vec<_>>();
        let gutter = (view.gutter_digits as f32 * 7.0 + 8.0).max(23.0);
        let rows = view.rows.iter().enumerate().filter_map(|(index, row)| {
            let first = row.line.tokens.first().map_or("", |token| token.text);
            if view.structured && row.line.kind == "meta" && !first.starts_with("@@") {
                return None;
            }
            let body = row
                .line
                .tokens
                .iter()
                .map(|token| token.text)
                .collect::<String>();
            if view.structured && first.starts_with("@@") {
                return Some(
                    div()
                        .px(px(7.0))
                        .line_height(px(24.0))
                        .text_color(palette.muted)
                        .bg(palette.subtle)
                        .child(body),
                );
            }
            let (background, color) = match row.line.kind {
                "add" => (palette.diff_add_bg, palette.success),
                "remove" => (palette.diff_remove_bg, palette.danger),
                _ => (palette.bg, palette.text),
            };
            let anchor = row
                .new_number
                .map(|number| ("新", number))
                .or_else(|| row.old_number.map(|number| ("旧", number)));
            let line = div()
                .flex()
                .items_center()
                .min_h(px(22.5))
                .line_height(px(22.5))
                .bg(background)
                .text_color(color)
                .when(view.structured, |line| {
                    line.children([row.old_number, row.new_number].map(|number| {
                        div()
                            .flex()
                            .justify_end()
                            .flex_none()
                            .w(px(gutter))
                            .pr(px(5.0))
                            .text_size(px(10.0))
                            .text_color(palette.muted)
                            .child(number.map(|number| number.to_string()).unwrap_or_default())
                    }))
                })
                .child(div().flex_none().pl(px(2.0)).pr(px(7.0)).child(body))
                .when_some(
                    anchor.zip(observed.as_ref()),
                    |line, ((side, number), observed)| {
                        let observed = Rc::clone(observed);
                        let scope = if staged { "已暂存" } else { "工作区" };
                        let location = format!("{} · {side}行 {number} · {scope}", diff.path);
                        line.child(
                            self.icon_button("line-feedback", "plus", 11.0)
                                .id(("line-feedback", index))
                                .ml_auto()
                                .mr_1()
                                .flex_none()
                                .h(px(22.0))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.open_comment(
                                        Rc::clone(&observed),
                                        staged,
                                        index,
                                        location.clone(),
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    },
                );
            Some(
                div().flex().flex_col().child(line).children(
                    feedback
                        .iter()
                        .filter(|item| {
                            item.diff
                                .as_ref()
                                .is_some_and(|anchor| anchor.index == index)
                        })
                        .map(|item| {
                            self.text(item.comment.clone(), 11.0, palette.text)
                                .whitespace_normal()
                                .bg(palette.subtle)
                                .px_2()
                                .py_1()
                        }),
                ),
            )
        });
        div()
            .id("diff-scroll")
            .overflow_scroll()
            .max_h(px(460.0))
            .border_t_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .font_family("Menlo")
                    .text_size(px(11.0))
                    .whitespace_nowrap()
                    .children(rows),
            )
            .into_any_element()
    }

    fn file_preview(&self, path: &str, source: &str) -> AnyElement {
        let copy = SharedString::from(source.to_owned());
        let lines = file_view::present(source, path, false);
        let width = lines.len().to_string().len();
        let body = div()
            .flex()
            .flex_col()
            .font_family("Menlo")
            .text_size(px(12.0))
            .line_height(px(22.8))
            .whitespace_nowrap()
            .text_color(self.palette.text)
            .children(lines.iter().map(|line| {
                self.file_line(line, line.number.map(|number| format!("{number:>width$}")))
            }));
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1()
                    .bg(self.palette.subtle)
                    .child(self.text("源码", 12.0, self.palette.muted))
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
            let color = self.palette.code_token(token.class);
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
