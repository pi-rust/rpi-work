//! Project and session lists and their device-local display preferences.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
};

use gpui::{
    App, Context, Div, FontWeight, IntoElement, Render, SharedString, Stateful, Window, div,
    prelude::*, px,
};
use serde_json::{Value, json};
use work_viewmodel::connection::normalize_address;

use super::{DraftKey, Shell};
use crate::{icons::icon, theme::Palette};

#[derive(Default)]
pub(super) struct Preferences {
    pub(super) flat: bool,
    collapsed_sections: BTreeSet<String>,
    collapsed_nodes: BTreeSet<String>,
    pinned_projects: BTreeMap<String, BTreeSet<String>>,
}

impl Preferences {
    pub(super) fn load(value: &Value) -> Self {
        Self {
            flat: value["layout"].as_str() == Some("flat"),
            collapsed_sections: serde_json::from_value(value["collapsedSections"].clone())
                .unwrap_or_default(),
            collapsed_nodes: serde_json::from_value(value["collapsedNodes"].clone())
                .unwrap_or_default(),
            pinned_projects: serde_json::from_value(value["pinnedProjects"].clone())
                .unwrap_or_default(),
        }
    }

    pub(super) fn value(&self) -> Value {
        json!({
            "layout": if self.flat { "flat" } else { "node" },
            "collapsedSections": self.collapsed_sections,
            "collapsedNodes": self.collapsed_nodes,
            "pinnedProjects": self.pinned_projects,
        })
    }

    fn pinned(&self, node: &str, project: &str) -> bool {
        self.pinned_projects
            .get(node)
            .is_some_and(|ids| ids.contains(project))
    }
}

#[derive(Clone, Copy)]
enum Section {
    Projects,
    Pinned,
    Recent,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Self::Projects => "projects",
            Self::Pinned => "pinned",
            Self::Recent => "recent",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Projects => "项目",
            Self::Pinned => "置顶",
            Self::Recent => "最近",
        }
    }
}

fn toggle(values: &mut BTreeSet<String>, value: &str) {
    if !values.remove(value) {
        values.insert(value.to_owned());
    }
}

struct Tooltip {
    label: SharedString,
    palette: Palette,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(self.palette.bg)
            .border_1()
            .border_color(self.palette.line)
            .rounded(px(6.0))
            .shadow_md()
            .px_2()
            .py_1()
            .text_size(px(12.0))
            .text_color(self.palette.text)
            .child(self.label.clone())
    }
}

impl Shell {
    fn navigation_node_key(&self) -> String {
        if self.node.is_none() {
            return "fixture".into();
        }
        if let Some(id) = &self.snapshot.node_id {
            return format!("node:{id}");
        }
        if let Ok(address) = normalize_address(&self.snapshot.node, false) {
            return format!("address:{address}");
        }
        // Older nodes may omit identity and be served under a URL prefix.
        // Keep those pins separate without persisting the raw endpoint.
        let mut hash = DefaultHasher::new();
        self.snapshot.node.hash(&mut hash);
        format!("endpoint:{:016x}", hash.finish())
    }

    fn navigation_node_label(&self) -> String {
        if self.node.is_none() {
            "示例节点".into()
        } else {
            normalize_address(&self.snapshot.node, false).unwrap_or_else(|_| "已连接节点".into())
        }
    }

    fn navigation_node_icon(&self) -> &'static str {
        match &self.node {
            None => "workspace",
            Some(node) if node.is_local() => "laptop",
            Some(_) => "globe",
        }
    }

    pub(super) fn navigation_tooltip(
        &self,
        element: Stateful<Div>,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let label = label.into();
        let palette = self.palette;
        element.tooltip(move |_, cx: &mut App| {
            cx.new(|_| Tooltip {
                label: label.clone(),
                palette,
            })
            .into()
        })
    }

    pub(super) fn navigation_keyboard_enabled(&self) -> bool {
        !self.search_input_open()
            && !self.project_form.open
            && !self.session_management.open()
            && self.question.is_none()
            && self.comment.is_none()
    }

    pub(super) fn reveal_project_navigation(&mut self) {
        let node = self.navigation_node_key();
        let navigation = &mut self.preferences.navigation;
        let section = navigation
            .collapsed_sections
            .remove(Section::Projects.key());
        let group = navigation.collapsed_nodes.remove(&node);
        if section || group {
            self.save_device_preferences();
        }
    }

    fn navigation_heading(&self, section: Section, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let key = section.key();
        let open = !self.preferences.navigation.collapsed_sections.contains(key);
        div()
            .flex()
            .items_center()
            .child(
                self.button(key, "")
                    .flex_1()
                    .justify_start()
                    .gap_1()
                    .tab_index(0)
                    .tab_stop(self.navigation_keyboard_enabled())
                    .focus(|button| button.bg(palette.hover))
                    .child(icon(
                        if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        12.0,
                        palette.muted,
                    ))
                    .child(self.text(section.label(), 12.0, palette.muted))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        toggle(&mut shell.preferences.navigation.collapsed_sections, key);
                        shell.project_menu = None;
                        shell.save_device_preferences();
                        cx.notify();
                    })),
            )
            .when(
                matches!(section, Section::Projects) && self.node.is_some(),
                |heading| heading.child(self.add_project_button(cx)),
            )
    }

    pub(super) fn navigation_lists(&self, cx: &Context<Self>) -> impl IntoElement {
        let node = self.navigation_node_key();
        let navigation = &self.preferences.navigation;
        let mut projects: Vec<_> = self.snapshot.projects.iter().enumerate().collect();
        projects.sort_by_key(|(_, project)| !navigation.pinned(&node, &project.id));
        let filter = self.session_filter.filter();
        let mut sessions: Vec<_> = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| !session.archived && filter.matches(session.activity))
            .collect();
        sessions.sort_by_key(|(_, session)| Reverse(session.updated_at));
        let (pinned, recent): (Vec<_>, Vec<_>) = sessions
            .into_iter()
            .partition(|(_, session)| session.pinned);
        let project_rows = || {
            projects
                .iter()
                .map(|&(index, _)| self.project_navigation_row(index, &node, cx))
        };
        let collapsed = |section: Section| navigation.collapsed_sections.contains(section.key());
        let node_open = !navigation.collapsed_nodes.contains(&node);
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .child(self.navigation_heading(Section::Projects, cx))
            .when(!collapsed(Section::Projects), |list| {
                if navigation.flat {
                    return list.children(project_rows());
                }
                list.when(!projects.is_empty(), |list| {
                    list.child(
                        self.button("project-node", "")
                            .w_full()
                            .justify_start()
                            .gap_1()
                            .pl_4()
                            .tab_index(0)
                            .tab_stop(self.navigation_keyboard_enabled())
                            .focus(|button| button.bg(palette.hover))
                            .child(icon(
                                if node_open {
                                    "chevron-down"
                                } else {
                                    "chevron-right"
                                },
                                12.0,
                                palette.muted,
                            ))
                            .child(icon(self.navigation_node_icon(), 13.0, palette.muted))
                            .child(
                                self.text(self.navigation_node_label(), 12.0, palette.text)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate(),
                            )
                            .child(self.text(projects.len().to_string(), 11.0, palette.muted))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                let node = shell.navigation_node_key();
                                toggle(&mut shell.preferences.navigation.collapsed_nodes, &node);
                                shell.project_menu = None;
                                shell.save_device_preferences();
                                cx.notify();
                            })),
                    )
                    .when(node_open, |list| list.children(project_rows()))
                })
            })
            .when(!pinned.is_empty(), |list| {
                list.child(self.navigation_heading(Section::Pinned, cx).mt_2())
                    .when(!collapsed(Section::Pinned), |list| {
                        list.children(
                            pinned
                                .iter()
                                .map(|&(index, session)| self.session_row(index, session, cx)),
                        )
                    })
            })
            .child(self.navigation_heading(Section::Recent, cx).mt_2())
            .children(self.session_filter_notice(!pinned.is_empty() || !recent.is_empty()))
            .when(!collapsed(Section::Recent), |list| {
                list.children(
                    recent
                        .iter()
                        .map(|&(index, session)| self.session_row(index, session, cx)),
                )
                .when(
                    recent.is_empty() && filter == super::session_filter::Filter::All,
                    |list| list.child(self.text("暂无最近会话", 12.0, palette.muted).px_3().py_1()),
                )
            })
    }

    fn project_navigation_row(&self, index: usize, node: &str, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let project = &self.snapshot.projects[index];
        let pinned = self.preferences.navigation.pinned(node, &project.id);
        let menu_open = self.project_menu == Some(index);
        let keyboard = self.navigation_keyboard_enabled();
        let subject = if project.path.is_empty() {
            &project.name
        } else {
            &project.path
        };
        let detail = format!("{} · {subject}", self.navigation_node_label());
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        self.navigation_tooltip(
                            self.button(("project", index), "")
                                .flex_1()
                                .min_w(px(0.0))
                                .pl(px(if self.preferences.navigation.flat {
                                    10.0
                                } else {
                                    25.0
                                }))
                                .justify_start()
                                .gap_2()
                                .h(px(30.0))
                                .tab_index(0)
                                .tab_stop(keyboard)
                                .focus(|button| button.bg(palette.hover))
                                .when(self.project == Some(index), |button| {
                                    button.bg(palette.hover)
                                })
                                .child(icon("folder", 15.0, palette.muted))
                                .child(
                                    self.text(project.name.clone(), 14.0, palette.text)
                                        .font_weight(FontWeight::MEDIUM)
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate(),
                                )
                                .when(pinned, |row| row.child(icon("pin", 13.0, palette.muted)))
                                .when(self.preferences.navigation.flat, |row| {
                                    row.child(icon(
                                        self.navigation_node_icon(),
                                        13.0,
                                        palette.muted,
                                    ))
                                }),
                            detail,
                        )
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.open_project_welcome(index, window, cx)
                            },
                        )),
                    )
                    .child(
                        self.navigation_tooltip(
                            self.button(("project-compose", index), "")
                                .w(px(25.0))
                                .h(px(30.0))
                                .px_0()
                                .tab_index(0)
                                .tab_stop(keyboard)
                                .focus(|button| button.bg(palette.hover))
                                .child(icon("compose", 13.0, palette.muted)),
                            format!("在 {} 中新建对话", project.name),
                        )
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.open_project_welcome(index, window, cx)
                            },
                        )),
                    )
                    .child(
                        self.navigation_tooltip(
                            self.button(("project-menu", index), "")
                                .w(px(25.0))
                                .h(px(30.0))
                                .px_0()
                                .tab_index(0)
                                .tab_stop(keyboard)
                                .focus(|button| button.bg(palette.hover))
                                .when(menu_open, |button| button.bg(palette.hover))
                                .child(icon("ellipsis", 15.0, palette.muted)),
                            format!("{} 项目操作", project.name),
                        )
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.project_menu =
                                (shell.project_menu != Some(index)).then_some(index);
                            cx.notify();
                        })),
                    ),
            )
            .when(menu_open, |row| {
                row.child(
                    self.button(
                        ("pin-project", index),
                        if pinned {
                            "取消置顶项目"
                        } else {
                            "置顶项目"
                        },
                    )
                    .justify_start()
                    .gap_2()
                    .ml_4()
                    .tab_index(0)
                    .tab_stop(keyboard)
                    .focus(|button| button.bg(palette.hover))
                    .child(icon("pin", 13.0, palette.muted))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        let node = shell.navigation_node_key();
                        let id = &shell.snapshot.projects[index].id;
                        let list = shell
                            .preferences
                            .navigation
                            .pinned_projects
                            .entry(node)
                            .or_default();
                        toggle(list, id);
                        window.focus_prev();
                        shell.project_menu = None;
                        shell.save_device_preferences();
                        cx.notify();
                    })),
                )
                .child(
                    self.button(("menu-new-conversation", index), "新建对话")
                        .justify_start()
                        .gap_2()
                        .ml_4()
                        .tab_index(0)
                        .tab_stop(keyboard)
                        .focus(|button| button.bg(palette.hover))
                        .child(icon("compose", 13.0, palette.muted))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.open_project_welcome(index, window, cx)
                        })),
                )
            })
    }

    pub(super) fn project_welcome(&self, index: usize, cx: &Context<Self>) -> Div {
        let palette = self.palette;
        let keyboard = self.navigation_keyboard_enabled() && self.overlay.is_none();
        let suggestions = [
            (
                "folder",
                "梳理项目结构",
                "梳理项目结构，列出主要入口和模块边界。",
            ),
            (
                "git-branch",
                "审阅当前变更",
                "审阅当前工作区变更，指出可能的回归。",
            ),
            (
                "list-checks",
                "检查测试覆盖",
                "找出当前功能缺少的测试覆盖。",
            ),
        ];
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .items_center()
            .justify_center()
            .gap_4()
            .px_4()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .gap_2()
                    .child(self.text("你想在", 22.0, palette.text))
                    .child(
                        self.button("welcome-project-picker", "")
                            .h(px(34.0))
                            .max_w(px(262.0))
                            .gap_2()
                            .bg(palette.hover)
                            .tab_index(0)
                            .tab_stop(keyboard)
                            .focus(|button| button.border_1().border_color(palette.strong_line))
                            .child(
                                self.text(
                                    self.snapshot.projects[index].name.clone(),
                                    22.0,
                                    palette.text,
                                )
                                .min_w(px(0.0))
                                .truncate(),
                            )
                            .child(icon("chevron-down", 14.0, palette.muted))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                shell.welcome_project_picker = !shell.welcome_project_picker;
                                cx.notify();
                            })),
                    )
                    .child(self.text("中构建什么？", 22.0, palette.text)),
            )
            .when(self.welcome_project_picker, |welcome| {
                welcome.child(
                    div()
                        .id("welcome-projects")
                        .w(px(260.0))
                        .max_h(px(140.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .p_1()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.line)
                        .children(self.snapshot.projects.iter().enumerate().map(
                            |(candidate, project)| {
                                self.button(("welcome-project", candidate), "")
                                    .justify_start()
                                    .gap_2()
                                    .tab_index(0)
                                    .tab_stop(keyboard)
                                    .when(candidate == index, |row| row.bg(palette.hover))
                                    .focus(|row| row.bg(palette.hover))
                                    .child(icon("folder", 13.0, palette.muted))
                                    .child(
                                        self.text(project.name.clone(), 13.0, palette.text)
                                            .min_w(px(0.0))
                                            .truncate(),
                                    )
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.open_project_welcome(candidate, window, cx)
                                    }))
                            },
                        )),
                )
            })
            .child(div().flex().flex_wrap().justify_center().gap_2().children(
                suggestions.into_iter().map(|(shape, label, text)| {
                    self.button(label, "")
                        .h(px(32.0))
                        .gap_2()
                        .border_1()
                        .border_color(palette.line)
                        .tab_index(0)
                        .tab_stop(keyboard)
                        .focus(|button| button.bg(palette.hover))
                        .child(icon(shape, 14.0, palette.muted))
                        .child(self.text(label, 13.0, palette.text))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            let draft = shell
                                .drafts
                                .entry(DraftKey::NewConversation(Some(index)))
                                .or_default();
                            draft.input.append_paragraphs(text);
                            shell.welcome_project_picker = false;
                            shell.focus_composer(window);
                            cx.notify();
                        }))
                }),
            ))
    }

    pub(super) fn open_project_welcome(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_menu = None;
        self.select_project(index, cx);
        if self.overlay == Some(super::Overlay::Navigation) {
            self.overlay = None;
        }
        self.focus_composer(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preferences_round_trip_with_node_scoped_pins() {
        let previous = Preferences::load(&Value::Null);
        assert!(!previous.flat);
        assert!(previous.collapsed_sections.is_empty());
        let stored = json!({
            "layout": "flat",
            "collapsedSections": ["recent"],
            "collapsedNodes": ["node:desktop"],
            "pinnedProjects": {"node:desktop": ["work"]},
        });
        let navigation = Preferences::load(&stored);
        assert!(navigation.pinned("node:desktop", "work"));
        assert!(!navigation.pinned("node:remote", "work"));
        assert_eq!(navigation.value(), stored);
    }
}
