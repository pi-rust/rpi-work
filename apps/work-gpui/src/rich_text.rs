//! Native layout for the shared Markdown presentation tree.

use std::ops::Range;

use gpui::{
    AnyElement, ClipboardItem, Div, FontStyle, FontWeight, HighlightStyle, InteractiveText,
    SharedString, StrikethroughStyle, StyledText, UnderlineStyle, div, prelude::*, px,
};
use work_viewmodel::conversation::markdown::{CodeToken, Element as Tag, MarkdownView, Node};

use super::Shell;
use crate::theme::Palette;

impl Shell {
    pub(super) fn rich_answer(&self, key: &str, view: &MarkdownView) -> impl IntoElement {
        let render = Markdown {
            palette: self.palette,
        };
        div()
            .id(SharedString::from(key.to_owned()))
            .min_w(px(0.0))
            .text_color(self.palette.text)
            .text_size(px(15.0))
            .line_height(px(25.0))
            .child(render.blocks(&view.nodes, 16.0))
            .when(view.plain, |answer| {
                answer.child(self.text("内容较复杂，已按原文显示。", 12.0, self.palette.muted))
            })
    }
}

struct Markdown {
    palette: Palette,
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
            Tag::Div => self.blocks(children, 16.0),
            Tag::P => div().child(self.inline(0, children)),
            Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6 => {
                let size = match tag {
                    Tag::H1 => 20.0,
                    Tag::H2 => 17.0,
                    _ => 16.0,
                };
                div()
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
        div().flex().flex_col().gap(px(4.0)).pl(px(3.0)).children(
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
        div()
            .id("table")
            .min_w(px(0.0))
            .max_h(px(560.0))
            .overflow_scroll()
            .rounded(px(9.0))
            .border_1()
            .border_color(self.palette.line)
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
                            .border_b_1()
                            .border_color(self.palette.line)
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
                                            cell.bg(self.palette.subtle)
                                                .font_weight(FontWeight::SEMIBOLD)
                                        })
                                        .child(self.inline(index, children)),
                                )
                            }))
                    })),
            )
    }

    fn code(&self, code: &str, language: &str, tokens: &[CodeToken]) -> impl IntoElement {
        let mut offset = 0;
        let highlights: Vec<_> = tokens
            .iter()
            .map(|token| {
                let start = offset;
                offset += token.text.len();
                (
                    start..offset,
                    HighlightStyle {
                        color: Some(self.palette.code_token(token.class)),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let code = SharedString::from(code.to_owned());
        let copy = code.clone();
        div()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded(px(9.0))
            .border_1()
            .border_color(self.palette.line)
            .bg(self.palette.subtle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(37.0))
                    .px(px(10.0))
                    .border_b_1()
                    .border_color(self.palette.line)
                    .text_size(px(11.0))
                    .text_color(self.palette.muted)
                    .child(language.to_owned())
                    .child(
                        div()
                            .id("copy-code")
                            .cursor_pointer()
                            .rounded(px(4.0))
                            .px_2()
                            .py_1()
                            .hover(|style| style.bg(self.palette.hover))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()))
                            })
                            .child("复制代码"),
                    ),
            )
            .child(
                div()
                    .id("code-scroll")
                    .overflow_scroll()
                    .max_h(px(560.0))
                    .p_3()
                    .font_family("Menlo")
                    .text_size(px(13.0))
                    .line_height(px(23.0))
                    .whitespace_nowrap()
                    .child(StyledText::new(code).with_highlights(highlights)),
            )
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
        let (ranges, urls): (Vec<_>, Vec<_>) = inline.links.into_iter().unzip();
        InteractiveText::new(("inline", id), text)
            .on_click(ranges, move |index, _, cx| cx.open_url(&urls[index]))
            .into_any_element()
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
