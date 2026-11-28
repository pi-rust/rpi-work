//! Search the current workspace catalog and open workbench actions.

use gpui::{
    Context, FocusHandle, FontWeight, MouseButton, ScrollHandle, Window, div, point, prelude::*, px,
};

use super::{DONATION_URL, Overlay, Shell, project_files::Tab};
use crate::icons::icon;

pub(super) struct Panel {
    pub(super) open: bool,
    close_focus: FocusHandle,
    result_focus: Vec<FocusHandle>,
    scroll: ScrollHandle,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> Self {
        Self {
            open: false,
            close_focus: cx.focus_handle(),
            result_focus: Vec::new(),
            scroll: ScrollHandle::new(),
        }
    }
}

#[derive(Clone, Copy)]
enum Target {
    Session(usize),
    Project(usize),
    NewConversation,
    Browser,
    Files,
    Settings,
    SettingsFile,
    Collaboration,
    Context,
    Donate,
}

struct ResultRow<'a> {
    target: Target,
    title: &'a str,
    detail: &'a str,
    glyph: &'static str,
}

impl Shell {
    pub(super) fn search_input_open(&self) -> bool {
        self.settings.open || self.workbench_search.open || self.resource_page_open()
    }

    pub(super) fn open_workbench_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.navigation_keyboard_enabled() || self.composer_is_marked() {
            return;
        }
        self.close_resource_page();
        if self.overlay == Some(super::Overlay::Navigation) {
            self.dismiss_drawer(window);
        }
        self.project_menu = None;
        self.close_session_filter_menu();
        self.composer_config.close_menu();
        self.welcome_project_picker = false;
        self.browser.restore_composer_focus();
        self.input_dragging = false;
        self.search.clear();
        self.workbench_search
            .scroll
            .set_offset(point(px(0.0), px(0.0)));
        self.workbench_search.open = true;
        self.search.focus.focus(window);
        cx.notify();
    }

    fn close_workbench_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workbench_search.open = false;
        self.input_dragging = false;
        self.search.clear();
        self.focus_composer(window);
        cx.notify();
    }

    fn workbench_search_results(&self) -> Vec<ResultRow<'_>> {
        let text = self.search.buffer.text.trim();
        let query = text
            .strip_prefix('>')
            .unwrap_or(text)
            .trim_start()
            .to_lowercase();
        let matches = |title: &str, detail: &str| {
            query.is_empty()
                || title.to_lowercase().contains(&query)
                || detail.to_lowercase().contains(&query)
        };
        let mut results = Vec::new();
        if !text.starts_with('>') {
            let mut sessions: Vec<_> = self
                .snapshot
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, session)| !session.archived)
                .collect();
            sessions.sort_by_key(|(_, session)| std::cmp::Reverse(session.updated_at));
            results.extend(sessions.into_iter().filter_map(|(index, session)| {
                let project = self
                    .snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == session.project_id);
                let title = session.display_title();
                let detail = project.map_or("", |project| project.name.as_str());
                matches(title, detail).then_some(ResultRow {
                    target: Target::Session(index),
                    title,
                    detail,
                    glyph: "message-circle",
                })
            }));
            results.extend(
                self.snapshot
                    .projects
                    .iter()
                    .enumerate()
                    .filter(|(_, project)| matches(&project.name, ""))
                    .map(|(index, project)| ResultRow {
                        target: Target::Project(index),
                        title: &project.name,
                        detail: "项目",
                        glyph: "folder",
                    }),
            );
        }
        results.extend(
            [
                (Target::NewConversation, "新对话", "plus"),
                (Target::Browser, "打开浏览器", "globe"),
                (Target::Files, "打开代码工作区", "code-xml"),
                (Target::Settings, "打开设置", "settings"),
                (Target::SettingsFile, "编辑设置 JSON", "file-text"),
                (Target::Collaboration, "打开工作协同", "globe"),
                (Target::Context, "查看上下文", "list-checks"),
                (Target::Donate, "捐赠", "thumbs-up"),
            ]
            .into_iter()
            .filter(|(target, title, _)| {
                (!matches!(target, Target::Context) || self.session.is_some())
                    && matches(
                        title,
                        match target {
                            Target::SettingsFile => "设置文件",
                            Target::Collaboration => "连接节点 远程 设备",
                            Target::Donate => "赞助 捐助 donate sponsor",
                            _ => "",
                        },
                    )
            })
            .map(|(target, title, glyph)| ResultRow {
                target,
                title,
                detail: "操作",
                glyph,
            }),
        );
        results
    }

    pub(super) fn activate_search(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.node_connection.open {
            if select {
                self.submit_node_connection(window, cx);
            }
        } else if self.node_share.open {
            if select {
                self.generate_node_qr(window, cx);
            }
        } else if self.provider_settings.editing() {
            if select {
                self.activate_provider_input(window, cx);
            }
        } else if self.chat_integrations.editing() {
            if select {
                self.finish_chat_field(window, cx);
            }
        } else if self.settings.open {
            self.activate_settings_search(select, window, cx);
        } else if self.resource_install_field_label().is_some() {
            if select {
                self.finish_resource_install_field(window, cx);
            }
        } else if self.workbench_search.open
            && self.search.buffer.is_committed()
            && let Some(target) = self
                .workbench_search_results()
                .first()
                .map(|row| row.target)
        {
            if select {
                self.select_workbench_result(target, window, cx);
            } else {
                if let Some(first) = self.workbench_search.result_focus.first() {
                    first.focus(window);
                    self.workbench_search.scroll.scroll_to_item(0);
                    cx.notify();
                }
            }
        }
    }

    fn select_workbench_result(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(target, Target::Session(index) if self.snapshot.sessions.get(index).is_none_or(|session| session.archived))
        {
            return;
        }
        self.close_workbench_search(window, cx);
        window.prevent_default();
        match target {
            Target::Session(index) => {
                self.dismiss_drawer(window);
                self.inspector_full = false;
                if self.session != Some(index) {
                    self.select_session(index, cx);
                }
            }
            Target::Project(index) => {
                self.dismiss_drawer(window);
                self.inspector_full = false;
                self.open_project_welcome(index, window, cx);
            }
            Target::NewConversation => self.open_new_conversation(window, cx),
            Target::Browser | Target::Files => {
                if window.viewport_size().width <= px(900.0) {
                    self.overlay = Some(Overlay::Inspector);
                } else {
                    self.overview_open = true;
                }
                if matches!(target, Target::Browser) {
                    self.open_browser_from_search(window, cx);
                } else {
                    self.show_project_tab(Tab::Files, cx);
                }
            }
            Target::Settings => self.open_settings(window, cx),
            Target::SettingsFile => self.edit_settings_file(window, cx),
            Target::Collaboration => self.open_collaboration_settings(window, cx),
            Target::Donate => cx.open_url(DONATION_URL),
            Target::Context => {
                // Let this key release finish before focusing the dialog's Close button.
                let revision = self.navigation_revision;
                cx.on_next_frame(window, move |shell, window, cx| {
                    if shell.navigation_revision == revision {
                        shell.open_session_context(window, cx);
                    }
                });
            }
        }
    }

    fn tab_workbench_search(&self, backwards: bool, window: &mut Window) {
        let panel = &self.workbench_search;
        let stops: Vec<_> = [&panel.close_focus, &self.search.focus]
            .into_iter()
            .chain(&panel.result_focus)
            .collect();
        let current = stops.iter().position(|focus| focus.is_focused(window));
        let count = stops.len();
        let next = match (backwards, current) {
            (true, Some(index)) => (index + count - 1) % count,
            (false, Some(index)) => (index + 1) % count,
            (true, None) => count - 1,
            (false, None) => 0,
        };
        stops[next].focus(window);
        if let Some(row) = next.checked_sub(2) {
            panel.scroll.scroll_to_item(row);
        }
    }

    pub(super) fn workbench_search_dialog(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let mut result_focus = std::mem::take(&mut self.workbench_search.result_focus);
        let results = self.workbench_search_results();
        result_focus.resize_with(results.len(), || cx.focus_handle());
        let rows: Vec<_> = results
            .into_iter()
            .zip(&result_focus)
            .enumerate()
            .map(|(index, (row, focus))| {
                self.button(("workbench-search-result", index), "")
                    .track_focus(focus)
                    .focus(|button| button.bg(palette.hover).border_color(palette.muted))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .h(px(38.0))
                    .gap_2()
                    .justify_start()
                    .child(icon(row.glyph, 15.0, palette.muted))
                    .child(
                        self.text(row.title.to_owned(), 14.0, palette.text)
                            .flex_1()
                            .min_w_0()
                            .truncate(),
                    )
                    .when(!row.detail.is_empty(), |button| {
                        button.child(
                            self.text(row.detail.to_owned(), 12.0, palette.muted)
                                .max_w(px(120.0))
                                .truncate(),
                        )
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.select_workbench_result(row.target, window, cx)
                    }))
            })
            .collect();
        self.workbench_search.result_focus = result_focus;
        div()
            .id("workbench-search-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000030))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .capture_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    let modifiers = event.keystroke.modifiers;
                    match event.keystroke.key.as_str() {
                        "escape" if !modifiers.modified() && !shell.composer_is_marked() => {
                            shell.close_workbench_search(window, cx);
                        }
                        "tab" if !modifiers.alt && !modifiers.control && !modifiers.platform => {
                            shell.tab_workbench_search(modifiers.shift, window);
                            window.prevent_default();
                            cx.notify();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("workbench-search-dialog")
                    .tab_group()
                    .w_full()
                    .max_w(px(550.0))
                    .max_h(px(
                        (f32::from(window.viewport_size().height) - 24.0).min(540.0)
                    ))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p(px(15.0))
                    .rounded(px(14.0))
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .flex_none()
                            .child(
                                self.text("搜索与操作", 16.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                self.icon_button("close-workbench-search", "x", 32.0)
                                    .track_focus(&self.workbench_search.close_focus)
                                    .border_1()
                                    .border_color(gpui::transparent_black())
                                    .focus(|button| {
                                        button.bg(palette.hover).border_color(palette.muted)
                                    })
                                    .on_click(cx.listener(|shell, _, window, cx| {
                                        shell.close_workbench_search(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("workbench-search-field")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .h(px(38.0))
                            .flex_none()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(if self.search.focus.is_focused(window) {
                                palette.muted
                            } else {
                                palette.line
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|shell, _, window, _| shell.search.focus.focus(window)),
                            )
                            .child(icon("search", 15.0, palette.muted))
                            .child(self.search_input_element(cx)),
                    )
                    .child(
                        div()
                            .id("workbench-search-results")
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.workbench_search.scroll)
                            .flex()
                            .flex_col()
                            .gap_1()
                            .when(rows.is_empty(), |list| {
                                list.child(
                                    self.text("没有匹配的结果。", 13.0, palette.muted).py_2(),
                                )
                            })
                            .children(rows),
                    ),
            )
    }
}
