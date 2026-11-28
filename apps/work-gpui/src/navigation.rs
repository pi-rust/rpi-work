//! Project and session lists and their device-local display preferences.

use std::{
    cell::Cell,
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, Div, ElementId, FocusHandle, FontWeight,
    IntoElement, MouseButton, Pixels, Point, Render, ScrollHandle, SharedString, Stateful, Window,
    anchored, div, point, prelude::*, px,
};
use serde_json::{Value, json};
use work_viewmodel::connection::normalize_address;

use super::{DraftKey, Overlay, Shell};
use crate::{data::SessionItem, icons::icon, theme::Palette};

const TREE_INSET: f32 = 7.0;
const TREE_STEP: f32 = 16.0;
const PROJECT_SESSION_PREVIEW: usize = 5;

pub(super) fn tree_inset(depth: usize) -> Pixels {
    px(TREE_INSET + TREE_STEP * depth as f32)
}

pub(super) struct Panel {
    pub(super) toggle: FocusHandle,
    pub(super) scope: FocusHandle,
    pub(super) first: FocusHandle,
    pub(super) last: FocusHandle,
    scroll: ScrollHandle,
    last_focus: Option<FocusHandle>,
    revealed: Rc<Cell<Option<(ElementId, usize, Pixels)>>>,
    project_expansion: BTreeMap<String, bool>,
    full_project_sessions: BTreeSet<String>,
}

impl Panel {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            toggle: cx.focus_handle(),
            scope: cx.focus_handle(),
            first: cx.focus_handle(),
            last: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            last_focus: None,
            revealed: Rc::default(),
            project_expansion: BTreeMap::new(),
            full_project_sessions: BTreeSet::new(),
        }
    }

    pub(super) fn reopen(&mut self) {
        self.revealed.set(None);
        self.last_focus = None;
    }

    pub(super) fn tab(&self, backwards: bool, window: &mut Window, cx: &App) {
        let edge = if backwards { &self.first } else { &self.last };
        if edge.is_focused(window) || !self.scope.contains_focused(window, cx) {
            (if backwards { &self.last } else { &self.first }).focus(window);
        } else if backwards {
            window.focus_prev();
        } else {
            window.focus_next();
        }
    }
}

#[derive(Default)]
pub(super) struct Preferences {
    pub(super) flat: bool,
    collapsed_sections: BTreeSet<String>,
    collapsed_nodes: BTreeSet<String>,
    pinned_projects: BTreeMap<String, BTreeSet<String>>,
    archived_projects: BTreeMap<String, BTreeSet<String>>,
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
            archived_projects: serde_json::from_value(value["archivedProjects"].clone())
                .unwrap_or_default(),
        }
    }

    pub(super) fn value(&self) -> Value {
        json!({
            "layout": if self.flat { "flat" } else { "node" },
            "collapsedSections": self.collapsed_sections,
            "collapsedNodes": self.collapsed_nodes,
            "pinnedProjects": self.pinned_projects,
            "archivedProjects": self.archived_projects,
        })
    }

    fn pinned(&self, node: &str, project: &str) -> bool {
        self.pinned_projects
            .get(node)
            .is_some_and(|ids| ids.contains(project))
    }

    fn archived(&self, node: &str, project: &str) -> bool {
        self.archived_projects
            .get(node)
            .is_some_and(|ids| ids.contains(project))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectAction {
    NewConversation,
    Rename,
    Pin,
    OpenFolder,
    CopyPath,
    NodeInfo,
    Archive,
}

impl ProjectAction {
    fn appearance(self, pinned: bool) -> (&'static str, &'static str) {
        match self {
            Self::NewConversation => ("compose", "新建对话"),
            Self::Rename => ("pencil", "重命名项目"),
            Self::Pin if pinned => ("pin", "取消置顶项目"),
            Self::Pin => ("pin", "置顶项目"),
            Self::OpenFolder => ("folder", "打开项目文件夹"),
            Self::CopyPath => ("copy", "复制项目路径"),
            Self::NodeInfo => ("info", "节点信息"),
            Self::Archive => ("archive-box", "归档项目"),
        }
    }
}

pub(super) struct ProjectMenu {
    index: usize,
    anchor: Point<Pixels>,
    trigger: FocusHandle,
    items: Vec<(ProjectAction, FocusHandle)>,
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
            .max_w(px(360.0))
            .px_2()
            .py_1()
            .text_size(px(12.0))
            .text_color(self.palette.text)
            .child(self.label.clone())
    }
}

impl Shell {
    pub(super) fn navigation_node_key(&self) -> String {
        if self.node.is_none() {
            return "fixture".into();
        }
        if self.snapshot.node == "rpi-work://local" {
            return "local".into();
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

    pub(super) fn navigation_node_label(&self) -> String {
        if self.node.is_none() {
            self.snapshot.mode.clone()
        } else if self.snapshot.node == "rpi-work://local" {
            "此电脑".into()
        } else {
            normalize_address(&self.snapshot.node, false).unwrap_or_else(|_| "已连接节点".into())
        }
    }

    pub(super) fn navigation_node_icon(&self) -> &'static str {
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
        !self.runtime_dialog_open()
            && !self.settings.open
            && !self.workbench_search.open
            && !self.session_context.open
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
                self.navigation_control(self.button(key, ""), true)
                    .flex_1()
                    .justify_start()
                    .pl(tree_inset(0))
                    .gap_1()
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
                        shell.close_session_filter_menu();
                        shell.save_device_preferences();
                        cx.notify();
                    })),
            )
            .when(
                matches!(section, Section::Projects) && self.node.is_some(),
                |heading| heading.child(self.add_project_button(cx)),
            )
    }

    pub(super) fn navigation_lists(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let current_focus = window.focused(cx);
        let focus_changed = self.navigation.last_focus != current_focus;
        self.navigation.last_focus = current_focus.clone();
        let scroll = self.navigation.scroll.clone();
        let revealed = self.navigation.revealed.clone();
        let rows = self.navigation_rows(cx);
        let candidates = [
            self.session
                .map(|index| ElementId::from(("project-session-row", index))),
            self.project
                .map(|index| ElementId::from(("project-row", index))),
            self.session
                .map(|index| ElementId::from(("session-row", index))),
        ];
        let selected = candidates.iter().flatten().find_map(|candidate| {
            rows.iter()
                .position(|(id, _)| id == candidate)
                .map(|index| (candidate.clone(), index))
        });
        let mut focus_index = None;
        let children: Vec<_> = window.with_id("navigation-row-focus", |window| {
            rows.into_iter()
                .enumerate()
                .map(|(index, (id, row))| {
                    let focus = window.use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle());
                    let focus = focus.read(cx);
                    if focus_changed
                        && current_focus
                            .as_ref()
                            .is_some_and(|current| focus.contains(current, window))
                    {
                        focus_index = Some(index);
                    }
                    div()
                        .id(id)
                        .track_focus(focus)
                        .tab_index(0)
                        .tab_stop(false)
                        .flex_none()
                        .child(row)
                })
                .collect()
        });
        if let Some(index) = focus_index {
            scroll.scroll_to_item(index);
        }
        div()
            .on_children_prepainted({
                let scroll = scroll.clone();
                move |_, window, cx| {
                    let target = selected
                        .as_ref()
                        .map(|(id, index)| (id.clone(), *index, scroll.bounds().size.height));
                    let active_changed = revealed.take() != target;
                    revealed.set(target);
                    if let Some((_, index)) = selected
                        .as_ref()
                        .filter(|_| active_changed && focus_index.is_none())
                    {
                        let viewport = scroll.bounds();
                        let offset = scroll.offset().y;
                        if scroll.bounds_for_item(*index).is_some_and(|row| {
                            row.top() + offset < viewport.top()
                                || row.bottom() + offset > viewport.bottom()
                        }) {
                            scroll.scroll_to_item(*index);
                            window.defer(cx, |window, _| window.refresh());
                        }
                    }
                }
            })
            .id("navigation-scroll")
            .on_scroll_wheel(cx.listener(|shell, _, window, cx| {
                if shell.dismiss_project_menu(window) {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|shell, _, _, cx| {
                    if shell.close_session_filter_menu() {
                        cx.notify();
                    }
                }),
            )
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .pt(px(12.0))
            .pb_1()
            .children(children)
    }

    fn navigation_rows(&self, cx: &Context<Self>) -> Vec<(ElementId, AnyElement)> {
        let node = self.navigation_node_key();
        let navigation = &self.preferences.navigation;
        let mut projects: Vec<_> = self
            .snapshot
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| !navigation.archived(&node, &project.id))
            .collect();
        projects.sort_by_key(|(_, project)| !navigation.pinned(&node, &project.id));
        let filter = self.session_filter.filter();
        let mut sessions: Vec<_> = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                !session.archived
                    && !navigation.archived(&node, &session.project_id)
                    && filter.matches(session.activity)
            })
            .collect();
        sessions.sort_by_key(|(_, session)| Reverse(session.updated_at));
        let mut by_project = BTreeMap::<&str, Vec<_>>::new();
        for &(index, session) in &sessions {
            by_project
                .entry(&session.project_id)
                .or_default()
                .push((index, session));
        }
        let (pinned, recent): (Vec<_>, Vec<_>) = sessions
            .into_iter()
            .partition(|(_, session)| session.pinned);
        let collapsed = |section: Section| navigation.collapsed_sections.contains(section.key());
        let node_open = !navigation.collapsed_nodes.contains(&node);
        let palette = self.palette;
        let mut rows = vec![(
            "projects-heading".into(),
            self.navigation_heading(Section::Projects, cx)
                .into_any_element(),
        )];
        if !collapsed(Section::Projects) {
            if !navigation.flat && !projects.is_empty() {
                let label = self.navigation_node_label();
                let button = self
                    .navigation_tooltip(
                        self.navigation_control(self.button("project-node", ""), true)
                            .w_full()
                            .justify_start()
                            .gap_1()
                            .pl(tree_inset(1))
                            .child(
                                icon(
                                    if node_open {
                                        "chevron-down"
                                    } else {
                                        "chevron-right"
                                    },
                                    12.0,
                                    palette.muted,
                                )
                                .flex_none(),
                            )
                            .child(
                                icon(self.navigation_node_icon(), 13.0, palette.muted).flex_none(),
                            )
                            .child(
                                self.text(label.clone(), 12.0, palette.text)
                                    .flex_1()
                                    .min_w_0()
                                    .truncate(),
                            )
                            .child(
                                self.text(projects.len().to_string(), 11.0, palette.muted)
                                    .flex_none(),
                            ),
                        label,
                    )
                    .on_click(cx.listener(|shell, _, _, cx| {
                        let node = shell.navigation_node_key();
                        toggle(&mut shell.preferences.navigation.collapsed_nodes, &node);
                        shell.project_menu = None;
                        shell.close_session_filter_menu();
                        shell.save_device_preferences();
                        cx.notify();
                    }));
                rows.push(("node-heading".into(), button.into_any_element()));
            }
            if navigation.flat || node_open {
                let depth = if navigation.flat { 1 } else { 2 };
                for (index, project) in projects {
                    let opened = self.project_sessions_open(index);
                    rows.push((
                        ("project-row", index).into(),
                        self.project_navigation_row(index, &node, depth, opened, cx)
                            .into_any_element(),
                    ));
                    if opened {
                        let sessions = by_project
                            .get(project.id.as_str())
                            .map_or(&[][..], Vec::as_slice);
                        self.project_session_rows(&mut rows, index, sessions, depth + 1, cx);
                    }
                }
            }
        }
        for (section, sessions) in [(Section::Pinned, &pinned), (Section::Recent, &recent)] {
            if matches!(section, Section::Pinned) && sessions.is_empty() {
                continue;
            }
            rows.push((
                section.key().into(),
                self.navigation_heading(section, cx)
                    .mt_2()
                    .into_any_element(),
            ));
            if matches!(section, Section::Recent) {
                if let Some(notice) =
                    self.session_filter_notice(!pinned.is_empty() || !recent.is_empty())
                {
                    rows.push(("filter-notice".into(), notice.into_any_element()));
                }
                if !collapsed(section)
                    && recent.is_empty()
                    && filter == super::session_filter::Filter::All
                {
                    rows.push((
                        "empty-recent".into(),
                        self.text("暂无最近会话", 12.0, palette.muted)
                            .pl(tree_inset(1))
                            .py_1()
                            .into_any_element(),
                    ));
                }
            }
            if !collapsed(section) {
                rows.extend(sessions.iter().map(|&(index, session)| {
                    (
                        ("session-row", index).into(),
                        self.session_row(index, session, 1, false, cx)
                            .into_any_element(),
                    )
                }));
            }
        }
        rows
    }

    fn project_sessions_open(&self, index: usize) -> bool {
        let id = &self.snapshot.projects[index].id;
        self.navigation
            .project_expansion
            .get(id)
            .copied()
            .unwrap_or(self.project == Some(index))
    }

    fn project_session_rows(
        &self,
        rows: &mut Vec<(ElementId, AnyElement)>,
        project: usize,
        sessions: &[(usize, &SessionItem)],
        depth: usize,
        cx: &Context<Self>,
    ) {
        if sessions.is_empty() {
            let label = if self.session_filter.filter() == super::session_filter::Filter::All {
                "暂无会话"
            } else {
                "没有符合筛选条件的会话"
            };
            rows.push((
                ("empty-project-sessions", project).into(),
                self.text(label, 12.0, self.palette.muted)
                    .pl(tree_inset(depth + 1))
                    .py_1()
                    .into_any_element(),
            ));
            return;
        }

        let id = &self.snapshot.projects[project].id;
        let show_all = self.navigation.full_project_sessions.contains(id);
        let mut hidden = 0;
        for (position, &(index, session)) in sessions.iter().enumerate() {
            let visible = position < PROJECT_SESSION_PREVIEW || self.session == Some(index);
            hidden += usize::from(!visible);
            if visible || show_all {
                rows.push((
                    ("project-session-row", index).into(),
                    self.session_row(index, session, depth, true, cx)
                        .into_any_element(),
                ));
            }
        }
        if hidden > 0 {
            let label = if show_all {
                "收起会话".into()
            } else {
                format!("显示更多（{hidden}）")
            };
            rows.push((
                ("project-sessions-more", project).into(),
                self.navigation_control(self.button(("show-more-sessions", project), ""), true)
                    .w_full()
                    .justify_start()
                    .pl(tree_inset(depth))
                    .gap_1()
                    .child(icon(
                        if show_all {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        12.0,
                        self.palette.muted,
                    ))
                    .child(self.text(label, 12.0, self.palette.muted))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        let id = &shell.snapshot.projects[project].id;
                        toggle(&mut shell.navigation.full_project_sessions, id);
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
    }

    pub(super) fn navigation_control(&self, button: Stateful<Div>, enabled: bool) -> Stateful<Div> {
        let palette = self.palette;
        button
            .tab_index(0)
            .tab_stop(enabled && self.navigation_keyboard_enabled())
            .focus(|button| button.bg(palette.hover).border_color(palette.muted))
            .border_1()
            .border_color(gpui::transparent_black())
    }

    fn project_navigation_row(
        &self,
        index: usize,
        node: &str,
        depth: usize,
        opened: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let project = &self.snapshot.projects[index];
        let navigation = &self.preferences.navigation;
        let pinned = navigation.pinned(node, &project.id);
        let menu_open = self
            .project_menu
            .as_ref()
            .is_some_and(|menu| menu.index == index);
        let node_label = self.navigation_node_label();
        let tooltip = format!(
            "{} {} 的会话 · {node_label}\n{}",
            if opened { "折叠" } else { "展开" },
            project.name,
            project.path,
        );
        let row = self
            .navigation_control(self.button(("project", index), ""), true)
            .flex_1()
            .min_w_0()
            .pl(tree_inset(depth))
            .justify_start()
            .gap_1()
            .h(px(30.0))
            .when(self.project == Some(index), |row| {
                row.bg(palette.subtle).border_color(palette.line)
            })
            .child(icon(
                if opened {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                12.0,
                palette.muted,
            ))
            .child(icon("folder", 14.0, palette.muted))
            .child(
                self.text(project.name.clone(), 13.0, palette.text)
                    .font_weight(FontWeight::MEDIUM)
                    .flex_1()
                    .min_w_0()
                    .truncate(),
            )
            .when(pinned, |row| row.child(icon("pin", 12.0, palette.muted)))
            .when(navigation.flat, |row| {
                row.child(icon(self.navigation_node_icon(), 12.0, palette.muted))
            });
        div()
            .id(("project-controls", index))
            .flex()
            .items_center()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                    shell.open_project_menu(index, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(self.navigation_tooltip(row, tooltip).on_click(cx.listener(
                move |shell, _, _, cx| {
                    let open = shell.project_sessions_open(index);
                    let id = shell.snapshot.projects[index].id.clone();
                    shell.navigation.project_expansion.insert(id, !open);
                    shell.project_menu = None;
                    cx.notify();
                },
            )))
            .child(
                self.navigation_tooltip(
                    self.navigation_control(self.button(("project-compose", index), ""), true)
                        .w(px(24.0))
                        .h(px(30.0))
                        .px_0()
                        .child(icon("plus", 13.0, palette.muted)),
                    format!("在 {} 中新建对话", project.name),
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.open_project_welcome(index, window, cx)
                })),
            )
            .child(
                self.navigation_tooltip(
                    self.navigation_control(self.button(("project-menu", index), ""), true)
                        .w(px(24.0))
                        .h(px(30.0))
                        .px_0()
                        .when(menu_open, |button| button.bg(palette.hover))
                        .child(icon("ellipsis", 14.0, palette.muted)),
                    format!("{} 项目操作", project.name),
                )
                .on_click(cx.listener(
                    move |shell, event: &ClickEvent, window, cx| {
                        if menu_open {
                            shell.dismiss_project_menu(window);
                            cx.notify();
                        } else {
                            shell.open_project_menu(index, event.position(), window, cx);
                        }
                    },
                )),
            )
    }

    fn open_project_menu(
        &mut self,
        index: usize,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = &self.snapshot.projects[index];
        let local = self.node.as_ref().is_some_and(|node| node.is_local());
        let path = std::path::Path::new(&project.path);
        let items: Vec<_> = [
            (ProjectAction::NewConversation, true),
            (ProjectAction::Rename, self.node.is_some()),
            (ProjectAction::Pin, true),
            (ProjectAction::OpenFolder, local && path.is_absolute()),
            (ProjectAction::CopyPath, !project.path.is_empty()),
            (ProjectAction::NodeInfo, self.node.is_some()),
            (ProjectAction::Archive, self.node.is_some()),
        ]
        .into_iter()
        .filter(|&(_, enabled)| enabled)
        .map(|(action, _)| (action, cx.focus_handle()))
        .collect();
        self.close_session_filter_menu();
        let trigger = window
            .focused(cx)
            .filter(|focus| self.navigation.scope.contains(focus, window))
            .unwrap_or_else(|| self.navigation.first.clone());
        items[0].1.focus(window);
        self.project_menu = Some(ProjectMenu {
            index,
            anchor,
            trigger,
            items,
        });
        cx.notify();
    }

    pub(super) fn project_menu_overlay(&self, cx: &Context<Self>) -> Option<gpui::Anchored> {
        let menu = self.project_menu.as_ref()?;
        let project = self.snapshot.projects.get(menu.index)?;
        let palette = self.palette;
        let pinned = self
            .preferences
            .navigation
            .pinned(&self.navigation_node_key(), &project.id);
        let index = menu.index;
        let popup = div()
            .id("project-context-menu")
            .flex()
            .flex_col()
            .w(px(208.0))
            .p_1()
            .bg(palette.bg)
            .border_1()
            .border_color(palette.line)
            .rounded(px(9.0))
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|shell, _, window, cx| {
                if shell.dismiss_project_menu(window) {
                    cx.notify();
                }
            }))
            .on_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    let backwards = match event.keystroke.key.as_str() {
                        "up" => true,
                        "down" => false,
                        "tab" => event.keystroke.modifiers.shift,
                        "escape" => {
                            shell.dismiss_project_menu(window);
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        _ => return,
                    };
                    if let Some(menu) = &shell.project_menu {
                        let current = menu
                            .items
                            .iter()
                            .position(|(_, focus)| focus.is_focused(window));
                        let len = menu.items.len();
                        let next = current.map_or(0, |i| {
                            if backwards {
                                (i + len - 1) % len
                            } else {
                                (i + 1) % len
                            }
                        });
                        menu.items[next].1.focus(window);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .children(
                menu.items
                    .iter()
                    .enumerate()
                    .map(|(position, &(action, ref focus))| {
                        let (shape, label) = action.appearance(pinned);
                        div()
                            .flex()
                            .flex_col()
                            .when(action == ProjectAction::Archive, |row| {
                                row.child(div().h(px(1.0)).mx_1().my_1().bg(palette.line))
                            })
                            .child(
                                self.navigation_control(
                                    self.button(("project-action", position), ""),
                                    true,
                                )
                                .w_full()
                                .h(px(30.0))
                                .justify_start()
                                .gap_2()
                                .track_focus(focus)
                                .child(icon(shape, 14.0, palette.muted))
                                .child(self.text(label, 13.0, palette.text))
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        shell.run_project_action(index, action, window, cx);
                                    },
                                )),
                            )
                    }),
            );
        Some(
            anchored()
                .position(menu.anchor)
                .offset(point(px(2.0), px(2.0)))
                .snap_to_window_with_margin(px(8.0))
                .child(popup),
        )
    }

    fn run_project_action(
        &mut self,
        index: usize,
        action: ProjectAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_project_menu(window);
        match action {
            ProjectAction::NewConversation => self.open_project_welcome(index, window, cx),
            ProjectAction::Rename => self.open_project_rename(index, window, cx),
            ProjectAction::Pin => {
                let node = self.navigation_node_key();
                let id = &self.snapshot.projects[index].id;
                toggle(
                    self.preferences
                        .navigation
                        .pinned_projects
                        .entry(node)
                        .or_default(),
                    id,
                );
                self.save_device_preferences();
            }
            ProjectAction::OpenFolder => {
                if self.node.as_ref().is_some_and(|node| node.is_local()) {
                    let path = std::path::Path::new(&self.snapshot.projects[index].path);
                    if path.is_absolute() {
                        cx.open_with_system(path);
                    }
                }
            }
            ProjectAction::CopyPath => cx.write_to_clipboard(ClipboardItem::new_string(
                self.snapshot.projects[index].path.clone(),
            )),
            ProjectAction::NodeInfo => {
                if self.overlay == Some(Overlay::Navigation) {
                    self.dismiss_drawer(window);
                }
                self.open_collaboration_settings(window, cx);
            }
            ProjectAction::Archive => {
                self.set_project_archived(index, true, cx);
                self.navigation.first.focus(window);
            }
        }
        cx.notify();
    }

    fn set_project_archived(&mut self, index: usize, archived: bool, cx: &mut Context<Self>) {
        let node = self.navigation_node_key();
        let id = &self.snapshot.projects[index].id;
        let projects = self
            .preferences
            .navigation
            .archived_projects
            .entry(node)
            .or_default();
        if archived {
            projects.insert(id.clone());
        } else {
            projects.remove(id);
        }
        self.save_device_preferences();
        cx.notify();
    }

    pub(super) fn archived_project_rows(&self, cx: &Context<Self>) -> Option<Div> {
        let node = self.navigation_node_key();
        let palette = self.palette;
        let mut projects = self
            .snapshot
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| self.preferences.navigation.archived(&node, &project.id))
            .peekable();
        projects.peek()?;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(self.text("已归档项目（本设备）", 13.0, palette.muted))
                .children(projects.map(|(index, project)| {
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .border_t_1()
                        .border_color(palette.line)
                        .child(icon("folder", 14.0, palette.muted))
                        .child(
                            self.text(project.name.clone(), 14.0, palette.text)
                                .flex_1()
                                .min_w_0()
                                .truncate(),
                        )
                        .child(
                            self.button(("restore-project", index), "恢复项目")
                                .tab_index(0)
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.focus_composer(window);
                                    shell.set_project_archived(index, false, cx);
                                })),
                        )
                })),
        )
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

    pub(super) fn dismiss_project_menu(&mut self, window: &mut Window) -> bool {
        let Some(menu) = self.project_menu.take() else {
            return false;
        };
        menu.trigger.focus(window);
        true
    }

    pub(super) fn open_project_welcome(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_menu = None;
        self.close_session_filter_menu();
        let id = self.snapshot.projects[index].id.clone();
        self.navigation.project_expansion.insert(id, true);
        self.select_project(index, cx);
        if self.overlay == Some(Overlay::Navigation) {
            self.overlay = None;
        }
        self.focus_composer(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preferences_round_trip_with_node_scoped_projects() {
        let previous = Preferences::load(&Value::Null);
        assert!(!previous.flat);
        assert!(previous.collapsed_sections.is_empty());
        assert!(!previous.archived("node:desktop", "work"));
        let stored = json!({
            "layout": "flat",
            "collapsedSections": ["recent"],
            "collapsedNodes": ["node:desktop"],
            "pinnedProjects": {"node:desktop": ["work"]},
            "archivedProjects": {"node:remote": ["work"]},
        });
        let navigation = Preferences::load(&stored);
        assert!(navigation.pinned("node:desktop", "work"));
        assert!(!navigation.pinned("node:remote", "work"));
        assert!(!navigation.archived("node:desktop", "work"));
        assert!(navigation.archived("node:remote", "work"));
        assert_eq!(navigation.value(), stored);
    }
}
