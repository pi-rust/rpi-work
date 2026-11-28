//! File reference completion uses the selected node's project directory API.

use std::ops::Range;

use gpui::{AnyElement, Context, IntoElement, Task, Window, div, prelude::*, px};
use work_client::{FileEntry, FileListing};

use crate::{icons::icon, prompt::FileReference};

use super::{
    DraftKey, Shell,
    project_files::{Contents, Route},
};

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    draft: DraftKey,
    project: String,
    directory: String,
}

#[derive(Default)]
pub(super) struct Panel {
    scope: Option<Scope>,
    request: Option<(Task<()>, tokio::task::AbortHandle)>,
    listing: Option<FileListing>,
    error: bool,
    selected: usize,
    dismissed: Option<(DraftKey, u64)>,
}

impl Panel {
    fn next_page(&self) -> Option<String> {
        self.listing
            .as_ref()?
            .pagination
            .as_ref()?
            .next_cursor
            .clone()
    }

    fn reset(&mut self) {
        if let Some((_, request)) = self.request.take() {
            request.abort();
        }
        self.scope = None;
        self.listing = None;
        self.error = false;
        self.selected = 0;
    }
}

fn query(text: &str, head: usize) -> Option<(Range<usize>, &str, &str)> {
    let prefix = text.get(..head)?;
    if text[head..].starts_with(|ch: char| !ch.is_whitespace()) {
        return None;
    }
    let quoted = prefix.rmatch_indices("@\"").find_map(|(start, _)| {
        let at_boundary = start == 0 || prefix[..start].ends_with(char::is_whitespace);
        let token = &prefix[start + 2..];
        (at_boundary && !token.contains('"')).then_some((start, token))
    });
    let (start, token) = match quoted {
        Some(reference) => reference,
        None => {
            let start = prefix.rfind(char::is_whitespace).map_or(0, |i| {
                i + prefix[i..].chars().next().map_or(0, char::len_utf8)
            });
            (start, prefix[start..].strip_prefix('@')?)
        }
    };
    if token.starts_with('/') || token.contains(['@', '\\', '"']) {
        return None;
    }
    let (directory, name) = token.rsplit_once('/').unwrap_or(("", token));
    if directory.split('/').any(|part| matches!(part, "." | "..")) {
        return None;
    }
    Some((start..head, directory, name))
}

impl Shell {
    fn reference_query(&self) -> Option<(Scope, Range<usize>, &str)> {
        if self.node.is_none()
            || !self.navigation_keyboard_enabled()
            || self.overlay.is_some()
            || self.composer_commands.add.is_some()
        {
            return None;
        }
        let draft = self.draft_key();
        let buffer = &self.drafts.get(&draft)?.input;
        if self.file_references.dismissed == Some((draft, buffer.revision)) {
            return None;
        }
        let (text, head) = buffer.completion_text()?;
        let (range, directory, name) = query(text, head)?;
        let project = self.snapshot.projects.get(self.project?)?.id.clone();
        Some((
            Scope {
                draft,
                project,
                directory: directory.into(),
            },
            range,
            name,
        ))
    }

    pub(super) fn sync_file_references(&mut self, window: &Window, cx: &mut Context<Self>) {
        let scope = self
            .focus
            .is_focused(window)
            .then(|| self.reference_query())
            .flatten()
            .map(|(scope, _, _)| scope);
        if self.file_references.scope == scope {
            return;
        }
        self.file_references.reset();
        self.file_references.scope = scope;
        self.read_reference_page(cx);
    }

    fn read_reference_page(&mut self, cx: &mut Context<Self>) {
        let panel = &mut self.file_references;
        if panel.request.is_some() {
            return;
        }
        let (Some(node), Some(scope)) = (self.node.as_ref(), panel.scope.clone()) else {
            return;
        };
        let cursor = panel.next_page();
        panel.error = false;
        let read = node.read_project(
            scope.project.clone(),
            Route::Directory {
                path: scope.directory.clone(),
                cursor: cursor.clone(),
            },
        );
        let request = read.abort_handle();
        let display = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.file_references;
                if panel.scope.as_ref() != Some(&scope) {
                    return;
                }
                panel.request = None;
                match result {
                    Ok(Ok(Contents::Directory(page))) => {
                        match panel.listing.as_mut().filter(|_| cursor.is_some()) {
                            Some(listing) => {
                                listing.entries.extend(page.entries);
                                listing.pagination = page.pagination;
                            }
                            None => panel.listing = Some(page),
                        }
                    }
                    _ => {
                        panel.listing = None;
                        panel.error = true;
                    }
                }
                cx.notify();
            });
        });
        self.file_references.request = Some((display, request));
        cx.notify();
    }

    fn file_reference_matches(&self) -> Option<(Range<usize>, Vec<&FileEntry>)> {
        let (scope, range, name) = self.reference_query()?;
        if self.file_references.scope.as_ref() != Some(&scope) {
            return None;
        }
        let name = name.to_lowercase();
        let entries = self
            .file_references
            .listing
            .iter()
            .flat_map(|listing| &listing.entries)
            .filter(|entry| {
                !entry.path.contains(['@', '\\', '"']) && entry.name.to_lowercase().contains(&name)
            })
            .take(6)
            .collect();
        Some((range, entries))
    }

    fn insert_file_reference(&mut self, path: &str, directory: bool, cx: &mut Context<Self>) {
        let Some((range, entries)) = self.file_reference_matches() else {
            return;
        };
        if !entries
            .iter()
            .any(|entry| entry.path == path && (entry.kind == "directory") == directory)
        {
            return;
        }
        let key = self.draft_key();
        let draft = self.drafts.entry(key).or_default();
        if directory {
            let value = if path.contains(char::is_whitespace) {
                format!("@\"{path}/")
            } else {
                format!("@{path}/")
            };
            draft.input.replace_completion(range, &value);
        } else if draft.add_file_reference(path) {
            draft.input.replace_completion(range, "");
        }
        self.file_references.selected = 0;
        cx.notify();
    }

    pub(super) fn file_reference_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some((_, entries)) = self.file_reference_matches() else {
            return false;
        };
        let len = entries.len();
        let index = self.file_references.selected.min(len.saturating_sub(1));
        match key {
            "up" if len > 0 => self.file_references.selected = (index + len - 1) % len,
            "down" if len > 0 => self.file_references.selected = (index + 1) % len,
            "tab" | "enter" if len > 0 => {
                let entry = entries[index];
                let (path, directory) = (entry.path.clone(), entry.kind == "directory");
                self.insert_file_reference(&path, directory, cx);
            }
            "tab" | "enter" if self.file_references.request.is_some() => {}
            "tab" | "enter"
                if self.file_references.error || self.file_references.next_page().is_some() =>
            {
                self.read_reference_page(cx);
            }
            "escape" => {
                let draft = self.draft_key();
                self.file_references.dismissed = self
                    .drafts
                    .get(&draft)
                    .map(|draft_state| (draft, draft_state.input.revision));
                self.file_references.reset();
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn file_reference_chips(
        &self,
        items: &[FileReference],
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if items.is_empty() {
            return None;
        }
        let palette = self.palette;
        let key = self.draft_key();
        Some(
            div()
                .id("file-reference-chips")
                .flex()
                .flex_wrap()
                .gap_1()
                .max_h(px(100.0))
                .overflow_y_scroll()
                .pb_2()
                .children(items.iter().map(|item| {
                    let id = item.id;
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .max_w_full()
                        .pl_2()
                        .rounded(px(5.0))
                        .bg(palette.subtle)
                        .child(icon("code-xml", 13.0, palette.muted).flex_none())
                        .child(
                            self.text(item.path.clone(), 12.0, palette.text)
                                .min_w_0()
                                .truncate(),
                        )
                        .child(
                            self.navigation_tooltip(
                                self.navigation_control(
                                    self.button(("remove-file-reference", id), "")
                                        .w(px(22.0))
                                        .px_0()
                                        .child(icon("x", 14.0, palette.muted)),
                                    self.overlay.is_none(),
                                )
                                .h(px(24.0))
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        if let Some(draft) = shell.drafts.get_mut(&key) {
                                            draft.references.retain(|item| item.id != id);
                                            draft.attachment_notice = None;
                                        }
                                        shell.focus_composer(window);
                                        cx.notify();
                                    },
                                )),
                                format!("移除文件引用 {}", item.path),
                            ),
                        )
                }))
                .into_any_element(),
        )
    }

    pub(super) fn file_reference_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let (_, entries) = self.file_reference_matches()?;
        let palette = self.palette;
        let selected = self
            .file_references
            .selected
            .min(entries.len().saturating_sub(1));
        let can_read_more = self.file_references.request.is_none()
            && (self.file_references.error
                || (entries.len() < 6 && self.file_references.next_page().is_some()));
        let status = if self.file_references.request.is_some() {
            Some("正在读取项目文件…")
        } else if self.file_references.error {
            Some("无法读取项目目录")
        } else if entries.is_empty() && self.file_references.next_page().is_none() {
            Some("当前目录没有匹配的文件")
        } else {
            None
        };
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .my_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(palette.line)
                .p_1()
                .when_some(status, |menu, status| {
                    menu.child(self.text(status, 12.0, palette.muted).p_1())
                })
                .children(entries.into_iter().enumerate().map(|(index, entry)| {
                    let path = entry.path.clone();
                    let directory = entry.kind == "directory";
                    let label = if directory {
                        format!("{path}/")
                    } else {
                        path.clone()
                    };
                    self.button(("file-reference", index), &label)
                        .justify_start()
                        .min_w_0()
                        .truncate()
                        .when(index == selected, |button| button.bg(palette.subtle))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.insert_file_reference(&path, directory, cx);
                            shell.focus_composer(window);
                        }))
                }))
                .when(can_read_more, |menu| {
                    menu.child(
                        self.button(
                            "reference-next-page",
                            if self.file_references.error {
                                "重试"
                            } else {
                                "加载更多"
                            },
                        )
                        .justify_start()
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.read_reference_page(cx);
                            shell.focus_composer(window);
                        })),
                    )
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::query;

    #[test]
    fn only_a_relative_reference_near_the_caret_opens_the_menu() {
        assert_eq!(query("查 @src/主", 12), Some((4..12, "src", "主")));
        assert_eq!(query("name@example.org", 16), None);
        assert_eq!(query("@../secret", 10), None);
        assert_eq!(query("@/tmp", 5), None);
    }
}
