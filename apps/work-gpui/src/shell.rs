//! Native workbench layout. The shared view model owns transcript ordering, tool
//! labels, code tokens and metrics.

use std::collections::{HashMap, HashSet};

use gpui::{
    AnyElement, Context, Div, FocusHandle, FontWeight, Hsla, KeyDownEvent, ScrollHandle,
    SharedString, Task, Window, WindowControlArea, div, point, prelude::*, px, relative,
};
use serde_json::Value;
use work_core::MessageBlock;
use work_viewmodel::conversation::transcript_view::{self, ActivityItem, Part, ToolView};
use work_viewmodel::conversation::{code_tokens, tool_display};
use work_viewmodel::observability::metrics::{percent, tokens};
use work_viewmodel::transcript::TranscriptOrder;

use crate::data::{Node, Snapshot};
use crate::theme::Palette;

const READING_WIDTH: f32 = 768.0;
const READING_INSET: f32 = 24.0;

enum Row {
    User(String),
    Assistant(String),
    Tool(ToolView),
}

pub struct Shell {
    snapshot: Snapshot,
    node: Option<Node>,
    loading: Option<(Task<()>, tokio::task::AbortHandle)>,
    load_error: Option<String>,
    palette: Palette,
    theme_id: String,
    dark: bool,
    navigation_open: bool,
    overview_open: bool,
    rows: Vec<Row>,
    project: Option<usize>,
    session: Option<usize>,
    transcript_session: Option<usize>,
    transcript_scroll: ScrollHandle,
    expanded: HashSet<String>,
    drafts: HashMap<Option<usize>, String>,
    focus: FocusHandle,
    order: TranscriptOrder,
    /// One-shot diagnostic so a headless launch can prove the first frame ran.
    traced: bool,
}

impl Shell {
    pub fn new(snapshot: Snapshot, node: Option<Node>, cx: &mut Context<Self>) -> Self {
        let fixture = node.is_none();
        let session = if fixture {
            snapshot
                .sessions
                .iter()
                .position(|session| session.id == "dark")
        } else {
            snapshot
                .sessions
                .iter()
                .enumerate()
                .max_by_key(|(_, session)| session.updated_at)
                .map(|(index, _)| index)
        };
        let project = match session {
            Some(index) => snapshot
                .projects
                .iter()
                .position(|project| project.id == snapshot.sessions[index].project_id),
            None => (!snapshot.projects.is_empty()).then_some(0),
        };
        let mut order = TranscriptOrder::default();
        let rows = present_rows(&mut order, "fixture", &snapshot.history, &snapshot.blocks);
        let mut shell = Self {
            session: session.filter(|_| fixture),
            transcript_session: session.filter(|_| fixture),
            transcript_scroll: ScrollHandle::new(),
            project,
            snapshot,
            node,
            loading: None,
            load_error: None,
            palette: Palette::from_preset("codex", false, false),
            theme_id: "codex".into(),
            dark: false,
            navigation_open: true,
            overview_open: true,
            rows,
            expanded: HashSet::new(),
            drafts: HashMap::new(),
            focus: cx.focus_handle(),
            order,
            traced: false,
        };
        if !fixture && session.is_some() {
            shell.change_session(session, cx);
        }
        shell
    }

    pub fn focus_composer(&self, window: &mut Window) {
        self.focus.focus(window);
    }

    fn typing(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.drafts.entry(self.session).or_default();
        match event.keystroke.key.as_str() {
            "backspace" => {
                draft.pop();
            }
            "enter" if event.keystroke.modifiers.shift => draft.push('\n'),
            "enter" => return,
            _ => {
                if let Some(text) = &event.keystroke.key_char {
                    draft.push_str(text);
                }
            }
        }
        cx.notify();
    }

    fn select_project(&mut self, index: usize, cx: &mut Context<Self>) {
        self.project = Some(index);
        let session = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.project_id == self.snapshot.projects[index].id)
            .max_by_key(|(_, session)| session.updated_at)
            .map(|(index, _)| index);
        self.change_session(session, cx);
    }

    fn select_session(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_session(Some(index), cx);
    }

    fn change_session(&mut self, session: Option<usize>, cx: &mut Context<Self>) {
        if self.session == session && self.load_error.is_none() {
            cx.notify();
            return;
        }
        self.session = session;
        self.transcript_scroll.set_offset(point(px(0.0), px(0.0)));
        self.expanded.clear();
        self.load_error = None;
        if let Some((_, request)) = self.loading.take() {
            request.abort();
        }
        if self.node.is_some() {
            self.transcript_session = None;
            self.rows.clear();
        }
        if let Some((node, index)) = self.node.as_ref().zip(session) {
            let session_id = self.snapshot.sessions[index].id.clone();
            let open = node.open(session_id);
            let request = open.abort_handle();
            let display = cx.spawn(async move |this, cx| {
                let result = match open.await {
                    Ok(result) => result.map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
                this.update(cx, |shell, cx| {
                    shell.loading = None;
                    match result {
                        Ok(connection) => {
                            shell.transcript_session = Some(index);
                            shell.rows = present_rows(
                                &mut shell.order,
                                connection.session_id(),
                                connection.messages(),
                                &connection.projection().blocks,
                            );
                            if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                                eprintln!("session loaded: {} transcript rows", shell.rows.len());
                            }
                        }
                        Err(error) => shell.load_error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
            self.loading = Some((display, request));
        }
        cx.notify();
    }

    /// Cycle to the next preset from the shared contract.
    fn next_theme(&mut self, cx: &mut Context<Self>) {
        let presets = Palette::presets();
        let index = presets
            .iter()
            .position(|(id, _)| *id == self.theme_id)
            .unwrap_or(0);
        self.theme_id = presets[(index + 1) % presets.len()].0.clone();
        self.palette = Palette::from_preset(&self.theme_id, self.dark, false);
        cx.notify();
    }

    fn toggle_dark(&mut self, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        self.palette = Palette::from_preset(&self.theme_id, self.dark, false);
        cx.notify();
    }

    fn toggle_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    // ---- render helpers -------------------------------------------------

    fn text(&self, value: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
        div()
            .text_size(px(size))
            .text_color(color)
            .child(value.into())
    }

    fn title(&self) -> &str {
        self.session
            .and_then(|index| self.snapshot.sessions.get(index))
            .map(|session| session.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or("工作台")
    }

    fn button(&self, id: &'static str, label: &str) -> gpui::Stateful<Div> {
        let palette = self.palette;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |style| style.bg(palette.hover))
            .child(self.text(label.to_owned(), 12.0, palette.muted))
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let projects = self
            .snapshot
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| {
                div()
                    .id(("project", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(31.0))
                    .px(px(10.0))
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .when(self.project == Some(index), |row| row.bg(palette.hover))
                    .hover(move |row| row.bg(palette.hover))
                    .child(self.text("▱", 15.0, palette.muted))
                    .child(
                        self.text(project.name.clone(), 14.0, palette.text)
                            .truncate(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.select_project(index, cx)))
            });
        let mut sessions: Vec<_> = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                self.project
                    .is_none_or(|index| self.snapshot.projects[index].id == session.project_id)
            })
            .collect();
        sessions.sort_by_key(|(_, session)| std::cmp::Reverse(session.updated_at));
        let sessions = sessions.into_iter().map(|(index, session)| {
            let selected = self.session == Some(index);
            let title = if session.title.is_empty() {
                "未命名会话"
            } else {
                &session.title
            };
            div()
                .id(("session", index))
                .flex()
                .items_center()
                .h(px(31.0))
                .px(px(10.0))
                .rounded(px(9.0))
                .cursor_pointer()
                .when(selected, |row| {
                    row.bg(palette.hover).font_weight(FontWeight::MEDIUM)
                })
                .hover(move |row| row.bg(palette.hover))
                .child(
                    self.text(
                        title.to_owned(),
                        14.0,
                        if selected {
                            palette.text
                        } else {
                            palette.muted
                        },
                    )
                    .truncate(),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.select_session(index, cx)))
        });

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(275.0))
            .px(px(9.0))
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(52.0))
                    .px(px(8.0))
                    .flex_none()
                    .child(
                        self.text("RPI Work", 16.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD),
                    ),
            )
            .child(
                div()
                    .id("navigation-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        self.text("项目", 12.0, palette.muted)
                            .px(px(10.0))
                            .pb(px(7.0)),
                    )
                    .children(projects)
                    .child(
                        self.text("最近会话", 12.0, palette.muted)
                            .px(px(10.0))
                            .pt(px(22.0))
                            .pb(px(7.0)),
                    )
                    .children(sessions),
            )
            .child(
                self.text(self.snapshot.mode.clone(), 11.0, palette.muted)
                    .px(px(10.0))
                    .py(px(12.0))
                    .truncate(),
            )
    }

    fn rail(&self, overview_available: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        div()
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .w(px(52.0))
            .py(px(10.0))
            .gap_2()
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line)
            .child(
                self.button("rail-navigation", "›_")
                    .bg(palette.hover)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigation_open = !this.navigation_open;
                        cx.notify();
                    })),
            )
            .when(overview_available, |rail| {
                rail.child(self.button("rail-project", "▱").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.overview_open = !this.overview_open;
                        cx.notify();
                    },
                )))
            })
            .child(div().flex_1())
            .child(
                self.button("appearance", if self.dark { "浅色" } else { "深色" })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_dark(cx))),
            )
    }

    fn header(&self, overview_available: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let preset = Palette::presets()
            .into_iter()
            .find(|(id, _)| id == &self.theme_id)
            .map(|(_, name)| name)
            .unwrap_or_default();
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(46.0))
            .bg(palette.titlebar)
            .border_b_1()
            .border_color(palette.line)
            .pr(px(12.0))
            .gap_2()
            .when(cfg!(target_os = "macos"), |bar| bar.pl(px(78.0)))
            .when(!cfg!(target_os = "macos"), |bar| bar.pl(px(12.0)))
            .child(
                self.button("navigation", "侧栏")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigation_open = !this.navigation_open;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        self.text(self.title().to_owned(), 14.0, palette.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate(),
                    ),
            )
            .child(
                self.button("theme", &preset)
                    .on_click(cx.listener(|this, _, _, cx| this.next_theme(cx))),
            )
            .when(overview_available, |header| {
                header.child(self.button("overview", "项目概览").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.overview_open = !this.overview_open;
                        cx.notify();
                    },
                )))
            })
    }

    fn overview(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let project = self
            .project
            .and_then(|index| self.snapshot.projects.get(index));
        let name = project.map_or("未选择项目", |project| project.name.as_str());
        let session_count = self
            .snapshot
            .sessions
            .iter()
            .filter(|session| project.is_some_and(|project| project.id == session.project_id))
            .count();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .bg(palette.bg)
            .border_l_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .px(px(15.0))
                    .border_b_1()
                    .border_color(palette.line)
                    .child(self.text("项目概览", 13.0, palette.text))
                    .child(self.button("close-overview", "×").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.overview_open = false;
                            cx.notify();
                        },
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p(px(28.0))
                    .child(
                        self.text(self.snapshot.mode.clone(), 11.0, palette.muted)
                            .truncate(),
                    )
                    .child(
                        self.text(name.to_owned(), 26.0, palette.text)
                            .font_weight(FontWeight::BOLD)
                            .truncate(),
                    )
                    .when(project.is_some(), |panel| {
                        panel.child(self.text(
                            format!("{session_count} 个会话"),
                            13.0,
                            palette.muted,
                        ))
                    })
                    .when_some(self.session, |panel, _| {
                        panel
                            .child(div().mt(px(13.0)).border_t_1().border_color(palette.line))
                            .child(self.text("当前会话", 12.0, palette.muted))
                            .child(
                                self.text(self.title().to_owned(), 14.0, palette.text)
                                    .truncate(),
                            )
                    }),
            )
    }

    fn transcript(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let show_rows = self.session.is_some() && self.session == self.transcript_session;
        let empty_text = match (self.loading.is_some(), self.session) {
            (true, _) => "正在读取会话…",
            (false, Some(_)) => "此会话暂无消息",
            (false, None) => "从侧栏选择会话",
        };
        let rows =
            self.rows
                .iter()
                .enumerate()
                .filter(|_| show_rows)
                .map(|(index, row)| match row {
                    Row::User(text) => div()
                        .flex()
                        .justify_end()
                        .child(
                            self.text(text.clone(), 15.0, palette.text)
                                .line_height(px(24.0))
                                .max_w(relative(0.86))
                                .px(px(15.0))
                                .py(px(10.0))
                                .rounded(px(19.0))
                                .bg(palette.subtle),
                        )
                        .into_any_element(),
                    Row::Assistant(text) => self
                        .text(text.clone(), 15.0, palette.text)
                        .line_height(px(25.0))
                        .into_any_element(),
                    Row::Tool(block) => self.tool_card(index, block, cx),
                });
        div()
            .id("transcript")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.transcript_scroll)
            .px(px(READING_INSET))
            .py(px(24.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(22.0))
                    .max_w(px(READING_WIDTH))
                    .w_full()
                    .mx_auto()
                    .children(rows)
                    .when_some(self.load_error.as_ref(), |column, error| {
                        column
                            .child(self.text(
                                format!("读取会话失败：{error}"),
                                14.0,
                                palette.danger,
                            ))
                            .child(self.button("retry", "重新读取").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.change_session(this.session, cx);
                                },
                            )))
                    })
                    .when(
                        self.load_error.is_none() && (!show_rows || self.rows.is_empty()),
                        |column| column.child(self.text(empty_text, 14.0, palette.muted)),
                    ),
            )
    }

    fn tool_card(&self, index: usize, block: &ToolView, cx: &mut Context<Self>) -> AnyElement {
        let palette = &self.palette;
        let ToolView {
            id,
            name,
            arguments,
            output,
            status,
            ..
        } = block;
        let open = self.expanded.contains(id);
        let target = tool_display::target(name, arguments);
        let glyph = match tool_display::icon(name) {
            "read" => "▤",
            "shell" => "❯",
            "edit" | "write" => "✎",
            "search" => "⌕",
            _ => "•",
        };
        let status_color = match *status {
            "completed" => palette.success,
            "failed" => palette.danger,
            _ => palette.muted,
        };
        let detail = if open {
            let mut detail = div()
                .flex()
                .flex_col()
                .gap_2()
                .pt(px(8.0))
                .border_1()
                .border_color(palette.line)
                .child(self.text(tool_detail_label(name), 11.0, palette.muted))
                .child(self.text(arguments.clone(), 12.0, palette.plain));
            if tool_display::is_read(name) {
                detail = detail
                    .child(self.text("工具结果", 11.0, palette.muted))
                    .child(self.code_block(output, &target));
            } else {
                detail = detail
                    .child(self.text("工具结果", 11.0, palette.muted))
                    .child(self.text(output.clone(), 12.0, palette.plain));
            }
            Some(detail)
        } else {
            None
        };
        div()
            .id(("tool", index))
            .flex()
            .flex_col()
            .rounded(px(6.0))
            .cursor_pointer()
            .hover({
                let background = palette.subtle;
                move |style| style.bg(background)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.0))
                    .px(px(6.0))
                    .gap_2()
                    .child(self.text(if open { "⌄" } else { "›" }, 13.0, palette.muted))
                    .child(self.text(glyph, 12.0, palette.muted))
                    .child(self.text(name.clone(), 12.0, palette.muted))
                    .child(
                        self.text(target, 12.0, palette.muted)
                            .min_w(px(0.0))
                            .truncate(),
                    )
                    .child(
                        div()
                            .size(px(5.0))
                            .flex_none()
                            .rounded(px(3.0))
                            .bg(status_color),
                    ),
            )
            .children(detail)
            .on_click(cx.listener({
                let id = id.clone();
                move |this, _, _, cx| this.toggle_tool(&id, cx)
            }))
            .into_any_element()
    }

    /// Code lines with lexer tokens from `work-viewmodel::conversation::code_tokens`.
    fn code_block(&self, source: &str, path: &str) -> impl IntoElement {
        let palette = &self.palette;
        let lines: Vec<_> = code_tokens::tokenize_file(source, path)
            .into_iter()
            .map(|line| {
                let spans: Vec<_> = line
                    .into_iter()
                    .map(|token| {
                        self.text(
                            token.text.to_string(),
                            12.5,
                            palette.code_token(token.class),
                        )
                    })
                    .collect();
                div().flex().flex_row().gap_1().children(spans)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(palette.bg)
            .border_1()
            .border_color(palette.line)
            .font_family("Menlo")
            .children(lines)
    }

    fn composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let (value, color) = self
            .drafts
            .get(&self.session)
            .filter(|draft| !draft.is_empty())
            .map_or_else(
                || ("要求后续更改".into(), palette.muted),
                |draft| (draft.clone(), palette.text),
            );
        let loaded_sample = self.node.is_none()
            && self.session.is_some()
            && self.session == self.transcript_session;
        let metrics = &self.snapshot.metrics;
        div().flex_none().px(px(READING_INSET)).pb(px(14.0)).child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(READING_WIDTH))
                .mx_auto()
                .child(
                    div()
                        .id("composer")
                        .flex()
                        .flex_col()
                        .bg(palette.bg)
                        .rounded(px(22.0))
                        .border_1()
                        .border_color(palette.strong_line)
                        .px(px(10.0))
                        .pt(px(12.0))
                        .pb(px(9.0))
                        .track_focus(&self.focus)
                        .on_click(cx.listener(|this, _, window, _| this.focus_composer(window)))
                        .on_key_down(cx.listener(Self::typing))
                        .child(
                            self.text(value, 15.0, color)
                                .px(px(4.0))
                                .min_h(px(47.0))
                                .max_h(px(126.0))
                                .overflow_hidden(),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .h(px(28.0))
                                .pl(px(4.0))
                                .child(self.text("只读 · 草稿保存在当前窗口", 12.0, palette.muted))
                                .child(
                                    self.text("↑", 15.0, palette.muted)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size(px(28.0))
                                        .rounded_full()
                                        .bg(palette.hover),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(px(32.0))
                        .px(px(6.0))
                        .gap_2()
                        .child(
                            self.text(self.snapshot.node.clone(), 11.0, palette.muted)
                                .truncate(),
                        )
                        .when(loaded_sample, |bar| {
                            bar.child(
                                self.text(
                                    format!(
                                        "↑ {}   ↓ {}   上下文 {}",
                                        tokens(metrics.input),
                                        tokens(metrics.output),
                                        percent(self.snapshot.context.percent)
                                    ),
                                    11.0,
                                    palette.muted,
                                )
                                .truncate(),
                            )
                        }),
                ),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.traced {
            self.traced = true;
            if std::env::var_os("RPI_GPUI_TRACE").is_some() {
                let tools = self
                    .rows
                    .iter()
                    .filter(|row| matches!(row, Row::Tool(_)))
                    .count();
                eprintln!(
                    "first frame: {} rows ({} transcript + {} tools), {} projects, {} sessions, metrics input={:?} context={:?}",
                    self.rows.len(),
                    self.rows.len() - tools,
                    tools,
                    self.snapshot.projects.len(),
                    self.snapshot.sessions.len(),
                    self.snapshot.metrics.input,
                    self.snapshot.context.percent,
                );
                for row in self.rows.iter().filter(|_| self.node.is_none()) {
                    eprintln!(
                        "  row {}",
                        match row {
                            Row::User(text) => format!("user {:?}", short(text)),
                            Row::Assistant(text) => format!("assistant {:?}", short(text)),
                            Row::Tool(block) => format!(
                                "tool {} {}",
                                block.name,
                                tool_display::target(&block.name, &block.arguments)
                            ),
                        }
                    );
                }
            }
        }
        let width = f32::from(window.viewport_size().width);
        let overview_available = width >= 1080.0;
        let overview_width = (width * 0.38).min(547.0);
        div()
            .flex()
            .flex_col()
            .size(relative(1.0))
            .bg(self.palette.bg)
            .child(self.header(overview_available, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.rail(overview_available, cx))
                    .when(self.navigation_open, |body| body.child(self.sidebar(cx)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.transcript(cx))
                            .child(self.composer(cx)),
                    )
                    .when(self.overview_open && overview_available, |body| {
                        body.child(self.overview(overview_width, cx))
                    }),
            )
    }
}

fn present_rows(
    order: &mut TranscriptOrder,
    scope: &str,
    history: &[Value],
    blocks: &[MessageBlock],
) -> Vec<Row> {
    let ordered = order.update(scope, history, blocks);
    let conversation =
        transcript_view::present(&ordered, history, blocks, None, None, Default::default());
    let mut rows = Vec::new();
    for part in conversation.parts {
        match part {
            Part::Message {
                role: "user", text, ..
            } => rows.push(Row::User(text)),
            Part::Message { text, .. } => rows.push(Row::Assistant(text)),
            Part::Activity { items, .. } => {
                rows.extend(items.into_iter().filter_map(|item| match item {
                    ActivityItem::Tool { block, .. } => Some(Row::Tool(block)),
                    ActivityItem::Note { .. } => None,
                }))
            }
        }
    }
    rows
}

fn tool_detail_label(name: &str) -> &'static str {
    if tool_display::is_read(name) {
        "调用参数 · 只读文件"
    } else if tool_display::is_shell(name) {
        "命令"
    } else {
        "调用参数"
    }
}

fn short(text: &str) -> String {
    let mut value: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        value.push('…');
    }
    value
}
