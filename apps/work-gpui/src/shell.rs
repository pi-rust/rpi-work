//! GPUI shell: the same workbench skeleton the Dioxus client renders, driven by
//! `work-viewmodel` for ordering, tool labels, code tokens and footer metrics.

use std::collections::HashSet;

use gpui::{
    AnyElement, Context, Div, FocusHandle, Hsla, KeyDownEvent, SharedString, Window, div,
    prelude::*, px, relative,
};
use work_core::{MessageBlock, ToolStatus};
use work_viewmodel::conversation::{code_tokens, tool_display};
use work_viewmodel::observability::metrics::{percent, tokens};
use work_viewmodel::transcript::{RowSource, TranscriptOrder};

use crate::data::Snapshot;
use crate::theme::Palette;

#[derive(Clone)]
enum Row {
    User(String),
    Assistant(String),
    Tool(MessageBlock),
}

pub struct Shell {
    snapshot: Snapshot,
    palette: Palette,
    rows: Vec<Row>,
    project: Option<usize>,
    session: Option<usize>,
    expanded: HashSet<String>,
    draft: String,
    focus: FocusHandle,
    /// Kept so the same scope keeps its display slots across re-renders.
    _order: TranscriptOrder,
    /// One-shot diagnostic so a headless launch can prove the first frame ran.
    traced: bool,
}

impl Shell {
    pub fn new(snapshot: Snapshot, cx: &mut Context<Self>) -> Self {
        let palette = Palette::codex_light();
        let mut order = TranscriptOrder::default();
        let ordered = order.update("prototype/dark", &snapshot.history, &snapshot.blocks);
        let rows = ordered
            .into_iter()
            .filter_map(|row| match row.source {
                RowSource::History(index) => {
                    let message = snapshot.history.get(index)?;
                    let text = message["content"].as_str().unwrap_or_default().to_string();
                    Some(if message["role"] == "user" {
                        Row::User(text)
                    } else {
                        Row::Assistant(text)
                    })
                }
                RowSource::Projection(index) => match snapshot.blocks.get(index)? {
                    block @ MessageBlock::ToolCall { .. } => Some(Row::Tool(block.clone())),
                    MessageBlock::Text { text, .. } | MessageBlock::PublicSummary { text, .. } => {
                        Some(Row::Assistant(text.clone()))
                    }
                    MessageBlock::Thinking { .. } => None,
                },
            })
            .collect();
        Self {
            session: snapshot
                .sessions
                .iter()
                .position(|session| session.id == "dark"),
            project: snapshot
                .projects
                .iter()
                .position(|project| project.id == "rpi-work"),
            snapshot,
            palette,
            rows,
            expanded: HashSet::new(),
            draft: String::new(),
            focus: cx.focus_handle(),
            _order: order,
            traced: false,
        }
    }

    pub fn focus_composer(&self, window: &mut Window) {
        self.focus.focus(window);
    }

    fn typing(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "backspace" => {
                self.draft.pop();
            }
            "enter" => {}
            _ => {
                if let Some(text) = event.keystroke.key_char.clone() {
                    self.draft.push_str(&text);
                }
            }
        }
        cx.notify();
    }

    fn select_project(&mut self, index: usize, cx: &mut Context<Self>) {
        self.project = Some(index);
        self.session = self.snapshot.sessions.iter().position(|session| {
            Some(&session.project_id)
                == self.snapshot.projects.get(index).map(|project| &project.id)
        });
        cx.notify();
    }

    fn select_session(&mut self, index: usize, cx: &mut Context<Self>) {
        self.session = Some(index);
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

    fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = &self.palette;
        let projects: Vec<_> = self
            .snapshot
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| {
                let selected = self.project == Some(index);
                div()
                    .id(("project", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .p(px(6.0))
                    .rounded(px(6.0))
                    .bg(if selected {
                        palette.accent.opacity(0.08)
                    } else {
                        palette.side
                    })
                    .text_size(px(13.0))
                    .text_color(if selected {
                        palette.text
                    } else {
                        palette.muted
                    })
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded(px(2.0))
                            .bg(palette.accent.opacity(if selected { 1.0 } else { 0.35 })),
                    )
                    .child(project.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_project(index, cx)))
            })
            .collect();
        let mut visible: Vec<(usize, &crate::data::SessionItem)> = self
            .snapshot
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| {
                self.project
                    .and_then(|index| self.snapshot.projects.get(index))
                    .is_some_and(|project| project.id == session.project_id)
            })
            .collect();
        // Same recency order the client uses for its sidebar.
        visible.sort_by_key(|(_, session)| std::cmp::Reverse(session.updated_at));
        let sessions: Vec<_> = visible
            .into_iter()
            .map(|(index, session)| {
                let selected = self.session == Some(index);
                div()
                    .id(("session", index))
                    .p(px(6.0))
                    .rounded(px(6.0))
                    .bg(if selected {
                        palette.accent.opacity(0.10)
                    } else {
                        palette.side
                    })
                    .text_size(px(13.0))
                    .text_color(if selected {
                        palette.text
                    } else {
                        palette.muted
                    })
                    .child(session.title.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_session(index, cx)))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .w(px(232.0))
            .p(px(12.0))
            .bg(palette.side)
            .border_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text("rpiwork", 15.0, palette.text))
                    .child(self.text("原型 · GPUI", 11.0, palette.muted)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text("项目", 11.0, palette.muted))
                    .children(projects),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text("会话", 11.0, palette.muted))
                    .children(sessions),
            )
            .child(div().flex_1())
            .child(self.text(self.snapshot.node.clone(), 10.0, palette.muted))
    }

    fn header(&mut self) -> impl IntoElement {
        let palette = &self.palette;
        let title = self
            .session
            .and_then(|index| self.snapshot.sessions.get(index))
            .map(|session| session.title.clone())
            .unwrap_or_else(|| "未选择会话".into());
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(44.0))
            .p(px(12.0))
            .border_1()
            .border_color(palette.line)
            .bg(palette.bg)
            .child(self.text(title, 14.0, palette.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.text("deepseek · deepseek-v4-pro", 12.0, palette.muted))
                    .child(div().size(px(7.0)).rounded(px(4.0)).bg(palette.success)),
            )
    }

    fn transcript(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let snapshot_rows = self.rows.clone();
        let rows: Vec<_> = snapshot_rows
            .iter()
            .enumerate()
            .map(|(index, row)| match row {
                Row::User(text) => div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text("你", 11.0, palette.muted))
                    .child(self.text(text.clone(), 15.0, palette.text))
                    .into_any_element(),
                Row::Assistant(text) => div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.text("rpi", 11.0, palette.muted))
                    .child(self.text(text.clone(), 15.0, palette.text))
                    .into_any_element(),
                Row::Tool(block) => self.tool_card(index, block, cx),
            })
            .collect();
        div()
            .id("transcript")
            .flex()
            .flex_col()
            .flex_1()
            .gap_3()
            .p(px(16.0))
            .overflow_y_scroll()
            .children(rows)
    }

    fn tool_card(
        &mut self,
        index: usize,
        block: &MessageBlock,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = &self.palette;
        let MessageBlock::ToolCall {
            id,
            name,
            arguments,
            output,
            status,
        } = block
        else {
            return div().into_any_element();
        };
        let open = self.expanded.contains(id);
        let target = tool_display::target(name, arguments);
        let glyph = match tool_display::icon(name) {
            "read" => "▤",
            "shell" => "❯",
            "edit" | "write" => "✎",
            "search" => "⌕",
            _ => "•",
        };
        let status_color = match status {
            ToolStatus::Completed => palette.success,
            ToolStatus::Failed => palette.danger,
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
            .p(px(8.0))
            .rounded(px(8.0))
            .bg(palette.card)
            .border_1()
            .border_color(palette.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.text(glyph, 12.0, palette.muted))
                    .child(self.text(name.clone(), 13.0, palette.text))
                    .child(self.text(target, 13.0, palette.muted))
                    .child(div().size(px(6.0)).rounded(px(3.0)).bg(status_color)),
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

    fn composer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = &self.palette;
        let body = if self.draft.is_empty() {
            self.text(
                "给 rpi 发消息…（原型：DPR 与 IME 尚未接入）",
                14.0,
                palette.muted,
            )
        } else {
            self.text(self.draft.clone(), 14.0, palette.text)
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(12.0))
            .m(px(12.0))
            .rounded(px(12.0))
            .bg(palette.card)
            .border_1()
            .border_color(palette.line)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::typing))
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.text("思考等级 高 · 工具 4", 11.0, palette.muted))
                    .child(self.text("原型：草稿只保存在内存", 11.0, palette.muted)),
            )
    }

    fn footer(&mut self) -> impl IntoElement {
        let palette = &self.palette;
        let metrics = &self.snapshot.metrics;
        let context = &self.snapshot.context;
        let metric = |label: &str, value: String| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(palette.muted)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(palette.text)
                        .child(value),
                )
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.0))
            .p(px(12.0))
            .bg(palette.side)
            .border_1()
            .border_color(palette.line)
            .child(metric("↑", tokens(metrics.input)))
            .child(metric("↓", tokens(metrics.output)))
            .child(metric("缓存", percent(metrics.cache_percent)))
            .child(metric(
                "成本",
                format!("${:.4}", metrics.cost.unwrap_or(0.0)),
            ))
            .child(metric("上下文", percent(context.percent)))
            .child(metric(
                "工具",
                format!(
                    "{} / 失败 {}",
                    self.snapshot.tools_loaded, self.snapshot.tools_failed
                ),
            ))
            .child(div().flex_1())
            .child(self.text(self.snapshot.mode.clone(), 11.0, palette.muted))
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                for row in &self.rows {
                    eprintln!(
                        "  row {}",
                        match row {
                            Row::User(text) => format!("user {:?}", short(text)),
                            Row::Assistant(text) => format!("assistant {:?}", short(text)),
                            Row::Tool(block) => match block {
                                MessageBlock::ToolCall {
                                    name, arguments, ..
                                } =>
                                    format!("tool {name} {}", tool_display::target(name, arguments)),
                                other => format!("block {:?}", other.id()),
                            },
                        }
                    );
                }
            }
        }
        let palette = &self.palette;
        div()
            .flex()
            .flex_row()
            .size(relative(1.0))
            .bg(palette.bg)
            .font_family("PingFang SC")
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(self.header())
                    .child(self.transcript(cx))
                    .child(self.composer(cx))
                    .child(self.footer()),
            )
    }
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
