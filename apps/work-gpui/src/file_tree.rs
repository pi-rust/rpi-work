use std::collections::HashMap;

use gpui::{Context, Div, SharedString, div, prelude::*, px};
use work_client::FileListing;

use super::{Contents, Pending, Route, Shell};
use crate::icons::icon;

#[derive(Default)]
pub(super) struct Tree {
    project: Option<String>,
    directories: HashMap<String, Directory>,
}

#[derive(Default)]
struct Directory {
    expanded: bool,
    listing: Option<FileListing>,
    pending: Option<Pending>,
    error: Option<String>,
}

impl Shell {
    pub(super) fn prepare_file_tree(&mut self, cx: &mut Context<Self>) {
        let project = self
            .project
            .and_then(|index| self.snapshot.projects.get(index));
        let Some(project) = project.filter(|_| self.node.is_some()) else {
            self.files.tree = Tree::default();
            return;
        };
        if self.files.tree.project.as_ref() == Some(&project.id) {
            return;
        }
        self.files.tree = Tree {
            project: Some(project.id.clone()),
            directories: HashMap::new(),
        };
        self.reload_file_tree(cx);
    }

    fn reload_file_tree(&mut self, cx: &mut Context<Self>) {
        self.files.tree.directories.clear();
        self.files
            .tree
            .directories
            .entry(String::new())
            .or_default()
            .expanded = true;
        self.load_tree_directory("", None, cx);
    }

    fn toggle_tree_directory(&mut self, path: &str, cx: &mut Context<Self>) {
        let directory = self
            .files
            .tree
            .directories
            .entry(path.to_owned())
            .or_default();
        directory.expanded = !directory.expanded;
        if directory.expanded && directory.pending.is_none() && directory.listing.is_none() {
            self.load_tree_directory(path, None, cx);
        }
        cx.notify();
    }

    fn load_tree_directory(&mut self, path: &str, cursor: Option<String>, cx: &mut Context<Self>) {
        let (Some(node), Some(project)) = (&self.node, self.files.tree.project.clone()) else {
            return;
        };
        let Some(directory) = self.files.tree.directories.get_mut(path) else {
            return;
        };
        if directory.pending.is_some() {
            return;
        }
        directory.error = None;
        let path = path.to_owned();
        let read = node.read_project(
            project.clone(),
            Route::Directory {
                path: path.clone(),
                cursor: cursor.clone(),
            },
        );
        let request = read.abort_handle();
        let display = cx.spawn(async move |this, cx| {
            let result = match read.await {
                Ok(Ok(Contents::Directory(listing))) => Ok(listing),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
                _ => unreachable!("directory reads only return listings"),
            };
            let _ = this.update(cx, |shell, cx| {
                if shell.files.tree.project.as_ref() != Some(&project) {
                    return;
                }
                let Some(directory) = shell.files.tree.directories.get_mut(&path) else {
                    return;
                };
                directory.pending = None;
                match result {
                    Ok(page) => match directory.listing.as_mut().filter(|_| cursor.is_some()) {
                        Some(listing) => {
                            listing.entries.extend(page.entries);
                            listing.pagination = page.pagination;
                        }
                        None => directory.listing = Some(page),
                    },
                    Err(error) => directory.error = Some(error),
                }
                cx.notify();
            });
        });
        directory.pending = Some(Pending {
            _display: display,
            request,
        });
        cx.notify();
    }

    pub(super) fn file_tree(
        &self,
        wide: bool,
        height: f32,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let keyboard = self.project_keyboard_enabled();
        let project = self
            .project
            .and_then(|index| self.snapshot.projects.get(index));
        let active = match &self.files.route {
            Some(Route::Source(path)) => Some(path.as_str()),
            _ => None,
        };
        div()
            .id("project-file-tree")
            .flex()
            .flex_col()
            .flex_none()
            .min_w_0()
            .border_color(palette.line)
            .when(wide, |tree| tree.w(px(152.0)).h_full().border_r_1())
            .when(!wide, |tree| {
                tree.w_full()
                    .max_h(px((height * 0.32).min(215.0)))
                    .border_b_1()
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .child(
                        self.text(
                            project
                                .map_or("项目文件", |project| &project.name)
                                .to_owned(),
                            12.0,
                            palette.text,
                        )
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                    )
                    .when(self.files.tree.project.is_some(), |header| {
                        header.child(
                            self.button("refresh-file-tree", "刷新")
                                .tab_index(0)
                                .tab_stop(keyboard)
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(|shell, _, _, cx| {
                                    shell.reload_file_tree(cx);
                                    if let Some(route @ Route::Source(_)) =
                                        shell.files.route.clone()
                                    {
                                        shell.read_project(Some(route), cx);
                                    }
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("project-file-tree-scroll")
                    .flex_auto()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .child(if project.is_none() {
                        self.project_note("请先选择项目。", false)
                    } else if self.node.is_none() {
                        self.project_note("连接节点后可读取项目文件。", false)
                    } else {
                        self.tree_directory("", 0, active, cx).into_any_element()
                    }),
            )
    }

    fn tree_directory(
        &self,
        path: &str,
        depth: usize,
        active: Option<&str>,
        cx: &Context<Self>,
    ) -> Div {
        let Some(directory) = self
            .files
            .tree
            .directories
            .get(path)
            .filter(|directory| directory.expanded)
        else {
            return div();
        };
        let palette = self.palette;
        let keyboard = self.project_keyboard_enabled();
        let listing = directory.listing.as_ref();
        let next = listing
            .and_then(|listing| listing.pagination.as_ref())
            .and_then(|page| page.next_cursor.clone());
        div()
            .children(
                listing
                    .into_iter()
                    .flat_map(|listing| &listing.entries)
                    .map(|entry| {
                        let entry_path = entry.path.clone();
                        let folder = entry.kind == "directory";
                        let open = folder
                            && self
                                .files
                                .tree
                                .directories
                                .get(&entry_path)
                                .is_some_and(|directory| directory.expanded);
                        let shape = if open {
                            "chevron-down"
                        } else if folder {
                            "chevron-right"
                        } else {
                            "code-xml"
                        };
                        let id = SharedString::from(format!("tree-entry:{entry_path}"));
                        div()
                            .child(
                                self.navigation_tooltip(
                                    self.button(id, "")
                                        .w_full()
                                        .min_w_0()
                                        .pl(px(6.0 + depth.min(6) as f32 * 12.0))
                                        .gap_1()
                                        .justify_start()
                                        .when(active == Some(entry_path.as_str()), |row| {
                                            row.bg(palette.subtle)
                                        })
                                        .tab_index(0)
                                        .tab_stop(keyboard)
                                        .focus(|row| row.bg(palette.hover))
                                        .child(div().flex_none().child(icon(
                                            shape,
                                            13.0,
                                            palette.muted,
                                        )))
                                        .child(
                                            self.text(entry.name.clone(), 12.0, palette.text)
                                                .min_w_0()
                                                .truncate(),
                                        )
                                        .on_click(cx.listener(move |shell, _, _, cx| {
                                            if folder {
                                                shell.toggle_tree_directory(&entry_path, cx);
                                            } else {
                                                shell.open_source(&entry_path, cx);
                                            }
                                        })),
                                    entry.path.clone(),
                                ),
                            )
                            .when(open, |branch| {
                                branch.child(self.tree_directory(
                                    &entry.path,
                                    depth + 1,
                                    active,
                                    cx,
                                ))
                            })
                    }),
            )
            .when(directory.pending.is_some(), |body| {
                body.child(self.text("正在读取…", 12.0, palette.muted).p_1())
            })
            .when_some(directory.error.as_ref(), |body, error| {
                body.child(self.project_note(error, true))
            })
            .when(
                listing.is_some_and(|listing| listing.entries.is_empty())
                    && directory.pending.is_none(),
                |body| body.child(self.text("目录为空。", 12.0, palette.muted).p_1()),
            )
            .when(
                directory.pending.is_none() && (next.is_some() || directory.error.is_some()),
                |body| {
                    let id = SharedString::from(format!("tree-page:{path}"));
                    let path = path.to_owned();
                    body.child(
                        self.button(
                            id,
                            if directory.error.is_some() {
                                "重试"
                            } else {
                                "加载更多"
                            },
                        )
                        .tab_index(0)
                        .tab_stop(keyboard)
                        .focus(|button| button.bg(palette.hover))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.load_tree_directory(&path, next.clone(), cx)
                        })),
                    )
                },
            )
    }
}
