//! Native layout for the shared Markdown presentation tree.

use std::{ops::Range, time::Duration};

use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Div, FontStyle, FontWeight, HighlightStyle,
    IntoElement, MouseButton, RenderOnce, ScrollHandle, SharedString, StrikethroughStyle,
    StyledText, Task, UnderlineStyle, Window, div, prelude::*, px,
};
use work_viewmodel::conversation::markdown::{CodeToken, Element as Tag, MarkdownView, Node};

use super::Shell;
use crate::theme::Palette;

impl Shell {
    pub(super) fn rich_answer(
        &self,
        key: &str,
        view: &MarkdownView,
        progress: bool,
    ) -> impl IntoElement {
        let render = Markdown {
            palette: self.palette,
            font_size: self.preferences.font_size(),
            code_numbers: self.preferences.code_numbers,
            keyboard: self.navigation_keyboard_enabled() && self.overlay.is_none(),
            spacing: if self.preferences.compact { 12.0 } else { 20.0 },
        };
        div()
            .id(SharedString::from(key.to_owned()))
            .min_w(px(0.0))
            .text_color(if progress {
                self.palette.muted
            } else {
                self.palette.text
            })
            .text_size(px(render.font_size))
            .line_height(px(self.preferences.line_height()))
            .child(render.blocks(&view.nodes, render.spacing))
            .when(view.plain, |answer| {
                answer.child(self.text("内容较复杂，已按原文显示。", 12.0, self.palette.muted))
            })
    }

    pub(super) fn tool_output(
        &self,
        id: &'static str,
        source: &str,
        path: Option<&str>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let mut display = String::new();
        let mut highlights = Vec::new();
        if let Some(path) = path {
            for (index, line) in
                work_viewmodel::conversation::code_tokens::tokenize_file(source, path)
                    .into_iter()
                    .enumerate()
            {
                if index > 0 {
                    display.push('\n');
                }
                for token in line {
                    let start = display.len();
                    display.push_str(token.text);
                    highlights.push((
                        start..display.len(),
                        HighlightStyle {
                            color: Some(palette.code_token(token.class)),
                            ..Default::default()
                        },
                    ));
                }
            }
        } else {
            display.push_str(source.strip_suffix('\n').unwrap_or(source));
        }
        ReadingScroll {
            id,
            keyboard: self.navigation_keyboard_enabled() && self.overlay.is_none(),
            focus: palette.muted,
            viewport: div()
                .flex()
                .items_start()
                .min_w_0()
                .max_h(px(320.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(gpui::transparent_black())
                .font_family("Menlo")
                .text_color(palette.text)
                .text_size(px(12.0))
                .line_height(px(19.0))
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_none()
                        .child(StyledText::new(display).with_highlights(highlights)),
                ),
        }
    }
}

struct Markdown {
    palette: Palette,
    font_size: f32,
    code_numbers: bool,
    keyboard: bool,
    spacing: f32,
}

impl Markdown {
    fn blocks(&self, nodes: &[Node], gap: f32) -> Div {
        let mut column = div().flex().flex_col().min_w(px(0.0)).gap(px(gap));
        let mut inline_start = 0;
        for (index, node) in nodes.iter().enumerate() {
            if let Some(block) = self.block(node) {
                if inline_start < index {
                    column = column.child(self.inline(inline_start, &nodes[inline_start..index]));
                }
                column = column.child(div().id(("block", index)).min_w(px(0.0)).child(block));
                inline_start = index + 1;
            }
        }
        if inline_start < nodes.len() {
            column = column.child(self.inline(inline_start, &nodes[inline_start..]));
        }
        column
    }

    fn block(&self, node: &Node) -> Option<AnyElement> {
        let (tag, children, start) = match node {
            Node::CodeBlock {
                code,
                language,
                tokens,
            } => {
                return Some(self.code(code, language, tokens).into_any_element());
            }
            Node::Element {
                tag,
                children,
                start,
            } => (*tag, children.as_slice(), *start),
            _ => return None,
        };
        let block = match tag {
            Tag::Div => self.blocks(children, self.spacing),
            Tag::P => div().child(self.inline(0, children)),
            Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6 => {
                let size = match tag {
                    Tag::H1 => self.font_size + 4.0,
                    Tag::H2 => self.font_size + 1.0,
                    _ => self.font_size,
                };
                div()
                    .mt(px(4.0))
                    .text_size(px(size))
                    .line_height(px(size * 1.45))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.inline(0, children))
            }
            Tag::Blockquote => div()
                .flex()
                .gap(px(14.0))
                .text_color(self.palette.muted)
                .child(div().w(px(2.0)).flex_none().bg(self.palette.line))
                .child(self.blocks(children, 12.0).flex_1()),
            Tag::Ul | Tag::Ol => self.list(children, start),
            Tag::Table => return Some(self.table(children).into_any_element()),
            Tag::Hr => div().h(px(1.0)).bg(self.palette.line),
            _ => return None,
        };
        Some(block.into_any_element())
    }

    fn list(&self, items: &[Node], start: Option<u64>) -> Div {
        div().flex().flex_col().gap(px(6.0)).pl(px(4.0)).children(
            items.iter().enumerate().filter_map(|(index, item)| {
                let Node::Element {
                    tag: Tag::Li,
                    children,
                    ..
                } = item
                else {
                    return None;
                };
                let task = matches!(children.first(), Some(Node::Task { .. }))
                    || matches!(children.first(), Some(Node::Element { tag: Tag::P, children, .. })
                        if matches!(children.first(), Some(Node::Task { .. })));
                let marker = if task {
                    String::new()
                } else {
                    start.map_or_else(
                        || "•".into(),
                        |first| format!("{}.", first.saturating_add(index as u64)),
                    )
                };
                Some(
                    div()
                        .id(("item", index))
                        .flex()
                        .gap(px(8.0))
                        .when(!task, |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .text_color(self.palette.muted)
                                    .child(marker),
                            )
                        })
                        .child(self.blocks(children, 4.0).flex_1()),
                )
            }),
        )
    }

    fn table(&self, sections: &[Node]) -> impl IntoElement {
        let rows: Vec<_> = sections
            .iter()
            .flat_map(|section| match section {
                Node::Element { children, .. } => children.as_slice(),
                _ => &[],
            })
            .filter_map(|row| match row {
                Node::Element {
                    tag: Tag::Tr,
                    children,
                    ..
                } => Some(children.as_slice()),
                _ => None,
            })
            .collect();
        let columns = rows.iter().map(|row| row.len()).max().unwrap_or(1);
        let row_count = rows.len();
        let border = self.palette.line;
        let focus = self.palette.muted;
        let viewport = div()
            .min_w(px(0.0))
            .max_h(px(560.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w(px(columns as f32 * 128.0))
                    .children(rows.into_iter().enumerate().map(|(row, cells)| {
                        div()
                            .id(("row", row))
                            .flex()
                            .when(row + 1 < row_count, |row| {
                                row.border_b_1().border_color(self.palette.line)
                            })
                            .when(row == 0, |row| {
                                row.rounded_t(px(8.0)).bg(self.palette.subtle)
                            })
                            .children(cells.iter().enumerate().filter_map(|(index, cell)| {
                                let Node::Element { tag, children, .. } = cell else {
                                    return None;
                                };
                                Some(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .px(px(10.0))
                                        .py(px(7.0))
                                        .text_size(px(13.0))
                                        .line_height(px(20.0))
                                        .when(*tag == Tag::Th, |cell| {
                                            cell.font_weight(FontWeight::SEMIBOLD)
                                        })
                                        .child(self.inline(index, children)),
                                )
                            }))
                    })),
            );
        ReadingScroll {
            id: "table",
            viewport,
            keyboard: self.keyboard,
            focus,
        }
    }

    fn code(&self, code: &str, language: &str, tokens: &[CodeToken]) -> impl IntoElement {
        let copy = SharedString::from(code.to_owned());
        let display = code.strip_suffix('\n').unwrap_or(code);
        let lines = if code.is_empty() {
            0
        } else {
            display.split('\n').count()
        };
        let numbers = (self.code_numbers && lines > 0 && lines <= 10_000).then(|| {
            (1..=lines)
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        });
        let mut offset = 0;
        let highlights: Vec<_> = tokens
            .iter()
            .filter_map(|token| {
                let start = offset;
                offset += token.text.len();
                let end = offset.min(display.len());
                (start < end).then_some((
                    start..end,
                    HighlightStyle {
                        color: Some(self.palette.code_token(token.class)),
                        ..Default::default()
                    },
                ))
            })
            .collect();
        let display = SharedString::from(display.to_owned());
        div()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded(px(10.0))
            .border_1()
            .border_color(self.palette.line)
            .bg(self.palette.subtle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(39.0))
                    .px(px(12.0))
                    .bg(self.palette.bg)
                    .border_b_1()
                    .border_color(self.palette.line)
                    .text_size(px(11.0))
                    .text_color(self.palette.muted)
                    .gap_2()
                    .child(div().min_w_0().truncate().child(language.to_owned()))
                    .child(CopyCode {
                        code: copy,
                        keyboard: self.keyboard,
                        palette: self.palette,
                    }),
            )
            .child(ReadingScroll {
                id: "code-scroll",
                keyboard: self.keyboard,
                focus: self.palette.muted,
                viewport: div()
                    .flex()
                    .items_start()
                    .max_h(px(560.0))
                    .rounded_b(px(9.0))
                    .border_2()
                    .border_color(gpui::transparent_black())
                    .px(px(13.0))
                    .py(px(10.0))
                    .font_family("Menlo")
                    .text_size(px(13.0))
                    .line_height(px(23.4))
                    .whitespace_nowrap()
                    .when_some(numbers, |row, numbers| {
                        row.child(
                            div()
                                .flex_none()
                                .pr(px(12.0))
                                .min_w(px(35.0))
                                .text_size(px(11.0))
                                .text_color(self.palette.muted)
                                .child(StyledText::new(numbers)),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .child(StyledText::new(display).with_highlights(highlights)),
                    ),
            })
    }

    fn inline(&self, id: usize, nodes: &[Node]) -> AnyElement {
        let mut inline = Inline::default();
        for node in nodes {
            inline.push(node, HighlightStyle::default(), self.palette);
        }
        let text = StyledText::new(inline.text).with_highlights(inline.highlights);
        if inline.links.is_empty() {
            return text.into_any_element();
        }
        let layout = text.layout().clone();
        div()
            .id(("inline", id))
            .min_w(px(0.0))
            .child(text)
            .on_click(move |event, _, cx| {
                let ClickEvent::Mouse(click) = event else {
                    return;
                };
                if click.down.button != MouseButton::Left || click.up.button != MouseButton::Left {
                    return;
                }
                let (Ok(down), Ok(up)) = (
                    layout.index_for_position(click.down.position),
                    layout.index_for_position(click.up.position),
                ) else {
                    return;
                };
                if let Some((_, url)) = inline
                    .links
                    .iter()
                    .find(|(range, _)| range.contains(&down) && range.contains(&up))
                {
                    cx.open_url(url);
                }
            })
            .into_any_element()
    }
}

#[derive(IntoElement)]
struct ReadingScroll {
    id: &'static str,
    viewport: Div,
    keyboard: bool,
    focus: gpui::Hsla,
}

impl RenderOnce for ReadingScroll {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id, cx, |_, _| ScrollHandle::new());
        let scroll = state.read(cx).clone();
        self.viewport
            .id(self.id)
            .overflow_scroll()
            .track_scroll(&scroll)
            .tab_index(0)
            .tab_stop(self.keyboard)
            .focus(|style| style.border_color(self.focus))
            .when(self.keyboard, |viewport| {
                viewport.on_key_down(move |event, window, cx| {
                    let key = &event.keystroke;
                    if key.modifiers.modified() {
                        return;
                    }
                    let max = scroll.max_offset();
                    let current = scroll.offset();
                    let mut next = current;
                    let line = px(24.0);
                    let page = (scroll.bounds().size.height - line).max(line);
                    match key.key.as_str() {
                        "left" => next.x += line,
                        "right" => next.x -= line,
                        "up" => next.y += line,
                        "down" => next.y -= line,
                        "pageup" => next.y += page,
                        "pagedown" => next.y -= page,
                        _ => return,
                    }
                    next.x = next.x.clamp(-max.width, px(0.0));
                    next.y = next.y.clamp(-max.height, px(0.0));
                    if next != current {
                        scroll.set_offset(next);
                        window.refresh();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
            })
    }
}

#[derive(IntoElement)]
struct CopyCode {
    code: SharedString,
    keyboard: bool,
    palette: Palette,
}

impl RenderOnce for CopyCode {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let reset = window.use_state(cx, |_, _| None::<Task<()>>);
        let copied = reset.read(cx).is_some();
        let palette = self.palette;
        div()
            .id("copy-code")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(68.0))
            .h(px(26.0))
            .tab_index(0)
            .tab_stop(self.keyboard)
            .cursor_pointer()
            .rounded(px(4.0))
            .hover(|style| style.bg(palette.hover))
            .focus(|style| style.bg(palette.hover))
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(self.code.to_string()));
                reset.update(cx, |reset, cx| {
                    let timer = cx.background_executor().timer(Duration::from_secs(2));
                    *reset = Some(cx.spawn(async move |state, cx| {
                        timer.await;
                        let _ = state.update(cx, |reset, cx| {
                            *reset = None;
                            cx.notify();
                        });
                    }));
                    cx.notify();
                });
            })
            .text_size(px(12.0))
            .child(if copied { "已复制" } else { "复制代码" })
    }
}

#[derive(Default)]
struct Inline {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    links: Vec<(Range<usize>, String)>,
}

impl Inline {
    fn push(&mut self, node: &Node, mut style: HighlightStyle, palette: Palette) {
        match node {
            Node::Text { text } => self.write(text, style),
            Node::Task { checked } => self.write(if *checked { "☑ " } else { "☐ " }, style),
            Node::Image { alt } => {
                style.color = Some(palette.muted);
                self.write(&format!("图片（未加载）：{alt}"), style);
            }
            Node::Link { href, children } => {
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.0),
                    ..Default::default()
                });
                let start = self.text.len();
                for child in children {
                    self.push(child, style, palette);
                }
                if let Some(url) = href
                    && self.text.len() > start
                {
                    self.links.push((start..self.text.len(), url.clone()));
                }
            }
            Node::Element { tag, children, .. } => {
                match tag {
                    Tag::Br => self.write("\n", style),
                    Tag::Em => style.font_style = Some(FontStyle::Italic),
                    Tag::Strong => style.font_weight = Some(FontWeight::SEMIBOLD),
                    Tag::Del => {
                        style.strikethrough = Some(StrikethroughStyle {
                            thickness: px(1.0),
                            color: None,
                        })
                    }
                    Tag::Code => style.background_color = Some(palette.subtle),
                    _ => {}
                }
                for child in children {
                    self.push(child, style, palette);
                }
            }
            Node::CodeBlock { code, .. } => self.write(code, style),
        }
    }

    fn write(&mut self, value: &str, style: HighlightStyle) {
        let start = self.text.len();
        self.text.push_str(value);
        if !value.is_empty() && style != HighlightStyle::default() {
            self.highlights.push((start..self.text.len(), style));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_viewmodel::conversation::markdown;

    #[test]
    fn inline_ranges_follow_utf8_and_only_validated_links_are_clickable() {
        let view = markdown::present(
            "中文 **加粗** [安全](https://example.com/) [原文](file:///tmp/x) ![示例](https://example.com/image.png)",
        );
        let Node::Element { children, .. } = &view.nodes[0] else {
            panic!("expected paragraph");
        };
        let palette = Palette::from_preset("codex", false, false);
        let mut inline = Inline::default();
        for node in children {
            inline.push(node, HighlightStyle::default(), palette);
        }
        assert_eq!(inline.links.len(), 1);
        assert_eq!(&inline.text[inline.links[0].0.clone()], "安全");
        assert_eq!(inline.links[0].1, "https://example.com/");
        assert!(inline.text.contains("原文 图片（未加载）：示例"));
        assert!(inline.highlights.iter().any(|(range, style)| {
            &inline.text[range.clone()] == "加粗" && style.font_weight == Some(FontWeight::SEMIBOLD)
        }));
    }
}
