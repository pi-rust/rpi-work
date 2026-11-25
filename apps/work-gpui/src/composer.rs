//! Native text editing and IME geometry for the workbench composer.

use std::ops::Range;

use gpui::{
    App, AvailableSpace, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, Element,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, GlobalElementId,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton, Pixels, Point,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
    div, fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::Shell;

#[derive(Default)]
pub(crate) struct Buffer {
    pub text: String,
    pub revision: u64,
    anchor: usize,
    head: usize,
    marked: Option<Range<usize>>,
}

impl Buffer {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    fn select(&mut self, head: usize, extend: bool) {
        self.head = head;
        if !extend {
            self.anchor = head;
        }
    }

    fn adjacent(&self, forward: bool) -> usize {
        if forward {
            self.text
                .grapheme_indices(true)
                .find(|(offset, _)| *offset > self.head)
                .map_or(self.text.len(), |(offset, _)| offset)
        } else {
            self.text
                .grapheme_indices(true)
                .rev()
                .find(|(offset, _)| *offset < self.head)
                .map_or(0, |(offset, _)| offset)
        }
    }

    pub fn can_send(&self) -> bool {
        self.marked.is_none() && !self.text.trim().is_empty()
    }

    pub fn finish_composition(&mut self) {
        self.marked = None;
    }

    pub fn replace(&mut self, utf16: Option<Range<usize>>, value: &str) {
        self.replace_range(utf16, value);
        self.finish_composition();
    }

    fn compose(
        &mut self,
        utf16: Option<Range<usize>>,
        value: &str,
        selected: Option<Range<usize>>,
    ) {
        let revision = self.revision;
        let inserted = self.replace_range(utf16, value);
        if let Some(selection) = selected {
            let selection = from_utf16(value, selection);
            self.anchor = inserted.start + selection.start;
            self.head = inserted.start + selection.end;
        }
        let marked = (!inserted.is_empty()).then_some(inserted);
        if self.revision == revision && self.marked != marked {
            self.revision = self.revision.wrapping_add(1);
        }
        self.marked = marked;
    }

    fn replace_range(&mut self, utf16: Option<Range<usize>>, value: &str) -> Range<usize> {
        let range = utf16
            .map(|range| from_utf16(&self.text, range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        if self.text[range.clone()] != *value {
            self.text.replace_range(range.clone(), value);
            self.revision = self.revision.wrapping_add(1);
        }
        let end = range.start + value.len();
        self.select(end, false);
        range.start..end
    }
}

fn char_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    (0..=offset)
        .rev()
        .find(|&byte| text.is_char_boundary(byte))
        .unwrap_or(0)
}

fn from_utf16(text: &str, range: Range<usize>) -> Range<usize> {
    let offset = |units| {
        let mut count = 0;
        for (byte, ch) in text.char_indices() {
            if count >= units {
                return byte;
            }
            count += ch.len_utf16();
        }
        text.len()
    };
    offset(range.start)..offset(range.end.max(range.start))
}

fn to_utf16(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start].encode_utf16().count();
    start..start + text[range].encode_utf16().count()
}

impl Shell {
    fn buffer(&self) -> Option<&Buffer> {
        self.drafts.get(&self.session).map(|draft| &draft.input)
    }

    fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.drafts.entry(self.session).or_default().input
    }

    pub(super) fn input_element(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .px(px(4.0))
            .text_size(px(15.0))
            .line_height(px(22.0))
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::input_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, event: &gpui::MouseDownEvent, window, cx| {
                    shell.focus_composer(window);
                    shell.select_at(event.position, event.modifiers.shift, cx);
                }),
            )
            .on_mouse_move(cx.listener(|shell, event: &gpui::MouseMoveEvent, _, cx| {
                if event.dragging() && shell.input_dragging {
                    shell.select_at(event.position, true, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.input_dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.input_dragging = false),
            )
            .on_scroll_wheel(cx.listener(|shell, event: &gpui::ScrollWheelEvent, _, cx| {
                if let Some(layout) = shell.input_layout.as_mut() {
                    let offset = (layout.scroll - event.delta.pixel_delta(layout.line_height).y)
                        .clamp(px(0.0), layout.max_scroll());
                    if offset != layout.scroll {
                        layout.scroll = offset;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }
            }))
            .child(InputElement(cx.entity()))
    }

    fn select_at(&mut self, position: Point<Pixels>, extend: bool, cx: &mut Context<Self>) {
        self.input_dragging = true;
        let Some(layout) = self.input_layout.as_ref() else {
            return;
        };
        let offset = layout.index_at(position);
        let buffer = self.buffer_mut();
        buffer.finish_composition();
        buffer.select(char_boundary(&buffer.text, offset), extend);
        cx.notify();
    }

    fn input_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if self.buffer().is_some_and(|input| input.marked.is_some()) {
            return;
        }
        let shortcut = if cfg!(target_os = "macos") {
            modifiers.platform && !modifiers.control
        } else {
            modifiers.control && !modifiers.platform
        };
        if shortcut && !modifiers.alt && !modifiers.shift {
            let buffer = self.buffer_mut();
            let selected = buffer.selection();
            match key {
                "a" => {
                    buffer.select(0, false);
                    buffer.select(buffer.text.len(), true);
                }
                "c" | "x" => {
                    if !selected.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            buffer.text[selected].into(),
                        ));
                        if key == "x" {
                            buffer.replace(None, "");
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        buffer.replace(None, &text.replace("\r\n", "\n").replace('\r', "\n"));
                    }
                }
                _ => return,
            }
        } else if !modifiers.platform && !modifiers.alt && !modifiers.control {
            if key == "enter" && !modifiers.shift {
                if !event.is_held {
                    self.send(cx);
                }
            } else {
                let vertical = self
                    .buffer()
                    .and_then(|buffer| self.input_layout.as_ref()?.vertical(buffer.head, key));
                let buffer = self.buffer_mut();
                let selected = buffer.selection();
                match key {
                    "enter" => buffer.replace(None, "\n"),
                    "left" | "right" => {
                        let forward = key == "right";
                        let offset = if modifiers.shift || selected.is_empty() {
                            buffer.adjacent(forward)
                        } else if forward {
                            selected.end
                        } else {
                            selected.start
                        };
                        buffer.select(offset, modifiers.shift);
                    }
                    "home" | "end" => {
                        let offset = if key == "home" {
                            buffer.text[..buffer.head]
                                .rfind('\n')
                                .map_or(0, |index| index + 1)
                        } else {
                            buffer.text[buffer.head..]
                                .find('\n')
                                .map_or(buffer.text.len(), |index| buffer.head + index)
                        };
                        buffer.select(offset, modifiers.shift);
                    }
                    "up" | "down" => {
                        if let Some(offset) = vertical {
                            let offset = char_boundary(&buffer.text, offset);
                            buffer.select(offset, modifiers.shift);
                        }
                    }
                    "backspace" | "delete" => {
                        if selected.is_empty() {
                            buffer.select(buffer.adjacent(key == "delete"), true);
                        }
                        buffer.replace(None, "");
                    }
                    _ => return,
                }
            }
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl EntityInputHandler for Shell {
    fn text_for_range(
        &mut self,
        utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.buffer().map_or("", |buffer| buffer.text.as_str());
        let range = from_utf16(text, utf16);
        *actual_range = Some(to_utf16(text, range.clone()));
        Some(text[range].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.buffer().map_or_else(
            || UTF16Selection {
                range: 0..0,
                reversed: false,
            },
            |buffer| UTF16Selection {
                range: to_utf16(&buffer.text, buffer.selection()),
                reversed: buffer.head < buffer.anchor,
            },
        ))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let buffer = self.buffer()?;
        Some(to_utf16(&buffer.text, buffer.marked.clone()?))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer_mut().finish_composition();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        utf16: Option<Range<usize>>,
        value: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer_mut().replace(utf16, value);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        utf16: Option<Range<usize>>,
        value: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer_mut().compose(utf16, value, selected);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.input_layout.as_ref()?;
        let text = self.buffer().map_or("", |buffer| buffer.text.as_str());
        let range = from_utf16(text, utf16);
        let start = layout.position(range.start.min(layout.len))?;
        let end = layout.position(range.end.min(layout.len))?;
        let width = if start.y == end.y {
            (end.x - start.x).abs().max(px(1.0))
        } else {
            px(1.0)
        };
        Some(Bounds::new(start, size(width, layout.line_height)))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.input_layout.as_ref()?;
        if !layout.bounds.contains(&position) {
            return None;
        }
        let text = self.buffer().map_or("", |buffer| buffer.text.as_str());
        let index = layout.index_at(position);
        Some(text.get(..index)?.encode_utf16().count())
    }
}

pub(super) struct Layout {
    lines: Vec<WrappedLine>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    height: Pixels,
    len: usize,
    scroll: Pixels,
    revision: u64,
    head: usize,
}

impl Layout {
    fn max_scroll(&self) -> Pixels {
        (self.height - self.bounds.size.height).max(px(0.0))
    }

    fn origin(&self) -> Point<Pixels> {
        self.bounds.origin - point(px(0.0), self.scroll)
    }

    fn index_at(&self, position: Point<Pixels>) -> usize {
        let mut position = position - self.origin();
        if position.y < px(0.0) {
            return 0;
        }
        let mut start = 0;
        for line in &self.lines {
            let height = line.size(self.line_height).height;
            if position.y < height {
                let index = line
                    .closest_index_for_position(position, self.line_height)
                    .unwrap_or_else(|index| index);
                return (start + index).min(self.len);
            }
            start += line.len() + 1;
            position.y -= height;
        }
        self.len
    }

    fn position(&self, index: usize) -> Option<Point<Pixels>> {
        let mut start = 0;
        let mut origin = self.origin();
        for line in &self.lines {
            if index <= start + line.len() {
                return Some(origin + line.position_for_index(index - start, self.line_height)?);
            }
            start += line.len() + 1;
            origin.y += line.size(self.line_height).height;
        }
        None
    }

    fn vertical(&self, offset: usize, key: &str) -> Option<usize> {
        let delta = match key {
            "up" => -self.line_height,
            "down" => self.line_height,
            _ => return None,
        };
        let mut position = self.position(offset)?;
        position.y += delta + self.line_height / 2.0;
        Some(self.index_at(position))
    }
}

struct InputElement(Entity<Shell>);

impl IntoElement for InputElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for InputElement {
    type RequestLayoutState = (SharedString, Vec<TextRun>, Pixels, Pixels);
    type PrepaintState = Option<Layout>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let shell = self.0.read(cx);
        let empty = Buffer::default();
        let buffer = shell.buffer().unwrap_or(&empty);
        let mut run = window.text_style().to_run(0);
        if buffer.text.is_empty() {
            run.color = shell.palette.muted;
        } else {
            run.color = shell.palette.text;
        }
        let text: SharedString = if buffer.text.is_empty() {
            "要求后续更改".into()
        } else {
            buffer.text.clone().into()
        };
        let selected = buffer.selection();
        let marked = buffer.marked.clone().unwrap_or(0..0);
        let mut edges = [
            0,
            selected.start,
            selected.end,
            marked.start,
            marked.end,
            text.len(),
        ];
        edges.sort_unstable();
        let runs: Vec<_> = edges
            .windows(2)
            .filter(|edge| edge[0] < edge[1])
            .map(|edge| {
                let start = edge[0];
                TextRun {
                    len: edge[1] - start,
                    background_color: selected.contains(&start).then_some(shell.palette.hover),
                    underline: marked.contains(&start).then_some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                }
            })
            .collect();
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        let id = window.request_measured_layout(style, {
            let text = text.clone();
            let runs = runs.clone();
            move |known, available, window, _| {
                let width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });
                let lines = shape(window, text.clone(), font_size, &runs, width);
                let height = text_height(&lines, line_height);
                size(width.unwrap_or(px(0.0)), height.clamp(px(47.0), px(126.0)))
            }
        });
        (id, (text, runs, font_size, line_height))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (text, runs, font_size, line_height): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let shell = self.0.read(cx);
        let empty = Buffer::default();
        let buffer = shell.buffer().unwrap_or(&empty);
        let line_height = *line_height;
        let lines = shape(
            window,
            text.clone(),
            *font_size,
            runs,
            Some(bounds.size.width),
        );
        let height = text_height(&lines, line_height);
        let mut layout = Layout {
            lines,
            bounds,
            line_height,
            height,
            len: buffer.text.len(),
            scroll: px(0.0),
            revision: buffer.revision,
            head: buffer.head,
        };
        let previous = shell.input_layout.as_ref();
        if let Some(previous) = previous {
            layout.scroll = previous.scroll.min(layout.max_scroll());
        }
        let follow = previous.is_none_or(|previous| {
            previous.revision != layout.revision
                || previous.head != layout.head
                || previous.bounds.size != bounds.size
        });
        if follow && let Some(caret) = layout.position(layout.head) {
            let above = (caret.y - bounds.top()).min(px(0.0));
            let below = (caret.y + line_height - bounds.bottom()).max(px(0.0));
            layout.scroll = (layout.scroll + above + below).clamp(px(0.0), layout.max_scroll());
        }
        Some(layout)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layout) = layout.take() else { return };
        let shell = self.0.read(cx);
        let focus = shell.focus.clone();
        let color = shell.palette.text;
        let caret = focus.is_focused(window)
            && shell
                .buffer()
                .is_none_or(|buffer| buffer.selection().is_empty());
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let mut origin = layout.origin();
            for line in &layout.lines {
                let height = line.size(layout.line_height).height;
                if origin.y + height > bounds.top() && origin.y < bounds.bottom() {
                    let _ = line.paint_background(
                        origin,
                        layout.line_height,
                        TextAlign::Left,
                        Some(bounds),
                        window,
                        cx,
                    );
                    let _ = line.paint(
                        origin,
                        layout.line_height,
                        TextAlign::Left,
                        Some(bounds),
                        window,
                        cx,
                    );
                }
                origin.y += height;
            }
            if caret {
                if let Some(position) = layout.position(layout.head) {
                    window.paint_quad(fill(
                        Bounds::new(position, size(px(1.0), layout.line_height)),
                        color,
                    ));
                }
            }
        });
        self.0
            .update(cx, |shell, _| shell.input_layout = Some(layout));
    }
}

fn shape(
    window: &Window,
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    width: Option<Pixels>,
) -> Vec<WrappedLine> {
    window
        .text_system()
        .shape_text(text, font_size, runs, width, None)
        .map(|lines| lines.into_vec())
        .unwrap_or_default()
}

fn text_height(lines: &[WrappedLine], line_height: Pixels) -> Pixels {
    line_height
        * lines
            .iter()
            .map(|line| line.wrap_boundaries().len() + 1)
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_offsets_are_relative_to_marked_text() {
        let mut input = Buffer::default();
        input.replace(None, "🌊前后");
        input.compose(Some(3..3), "拼🌊", Some(1..3));
        assert_eq!(input.text, "🌊前拼🌊后");
        assert_eq!(to_utf16(&input.text, input.selection()), 4..6);
        assert!(!input.can_send());
        input.compose(None, "拼音", None);
        input.replace(None, "输入法");
        assert_eq!(input.text, "🌊前输入法后");
        assert!(input.can_send());
    }

    #[test]
    fn marking_existing_text_protects_it_from_a_late_acknowledgement() {
        let mut draft = crate::prompt::Draft::default();
        draft.input.replace(None, "中文");
        draft.begin();
        draft.input.compose(Some(0..2), "中文", None);
        draft.accept();
        assert_eq!(draft.input.text, "中文");
        assert!(!draft.input.can_send());
    }

    #[test]
    fn deletion_keeps_extended_graphemes_intact() {
        let mut input = Buffer::default();
        input.replace(None, "中👩🏽‍💻e\u{301}");
        input.select(input.adjacent(false), true);
        input.replace(None, "");
        assert_eq!(input.text, "中👩🏽‍💻");
        input.select(input.adjacent(false), true);
        input.replace(None, "");
        assert_eq!(input.text, "中");
    }
}
