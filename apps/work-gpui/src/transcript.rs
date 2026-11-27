//! In-window reading state for the native transcript.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{App, Bounds, Pixels, ScrollHandle, Window, point, px};

#[derive(Clone, Debug, PartialEq)]
enum Position {
    Bottom,
    Offset(Pixels),
    Anchor {
        key: String,
        within_row: Pixels,
        offset: Pixels,
    },
}

#[derive(Default, PartialEq)]
struct Geometry {
    rows: Vec<(String, Pixels)>,
    max: Pixels,
}

impl Geometry {
    fn position(&self, offset: Pixels) -> Position {
        if self.max + offset <= px(24.0) {
            return Position::Bottom;
        }
        let index = self.rows.partition_point(|(_, top)| *top <= -offset);
        self.rows
            .get(index.saturating_sub(1))
            .map_or(Position::Offset(offset), |(key, top)| Position::Anchor {
                key: key.clone(),
                within_row: *top + offset,
                offset,
            })
    }

    fn offset(&self, position: Position) -> Pixels {
        let offset = match position {
            Position::Bottom => -self.max,
            Position::Offset(offset) => offset,
            Position::Anchor {
                key,
                within_row,
                offset,
            } => self
                .rows
                .iter()
                .find(|(candidate, _)| candidate == &key)
                .map_or(offset, |(_, top)| within_row - *top),
        };
        offset.clamp(-self.max, px(0.0))
    }
}

#[derive(Default)]
struct Viewport {
    geometry: Option<Geometry>,
    restore: Option<Position>,
    last_layout_offset: Option<Pixels>,
    revision: u64,
}

#[derive(Default)]
pub(super) struct Reading {
    viewport: Rc<RefCell<Viewport>>,
    positions: HashMap<usize, Position>,
    open: HashMap<usize, HashMap<String, bool>>,
}

impl Reading {
    pub(super) fn switch(
        &mut self,
        previous: Option<usize>,
        next: Option<usize>,
        scroll: &ScrollHandle,
        start_at_bottom: bool,
    ) {
        let viewport = self.viewport.borrow();
        if let Some(previous) = previous {
            let position = viewport.restore.clone().or_else(|| {
                viewport
                    .geometry
                    .as_ref()
                    .map(|geometry| geometry.position(scroll.offset().y))
            });
            if let Some(position) = position {
                self.positions.insert(previous, position);
            }
        }
        drop(viewport);
        self.viewport = Rc::new(RefCell::new(Viewport {
            restore: next.map(|session| {
                self.positions
                    .get(&session)
                    .cloned()
                    .unwrap_or(if start_at_bottom {
                        Position::Bottom
                    } else {
                        Position::Offset(px(0.0))
                    })
            }),
            ..Viewport::default()
        }));
    }

    pub(super) fn is_open(&self, session: Option<usize>, key: &str, default: bool) -> bool {
        session
            .and_then(|session| self.open.get(&session))
            .and_then(|open| open.get(key))
            .copied()
            .unwrap_or(default)
    }

    pub(super) fn set_open(&mut self, session: Option<usize>, key: String, open: bool) {
        if let Some(session) = session {
            self.open.entry(session).or_default().insert(key, open);
        }
    }

    pub(super) fn layout(
        &self,
        scroll: ScrollHandle,
        keys: Vec<String>,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let viewport = Rc::clone(&self.viewport);
        let offset_before_layout = scroll.offset().y;
        move |bounds, window, cx| {
            if keys.is_empty() {
                return;
            }
            let mut state = viewport.borrow_mut();
            let current = scroll.offset().y;
            // GPUI reports child bounds with the scroll offset applied.
            let content_top = scroll.bounds().top() + current;
            let geometry = Geometry {
                rows: keys
                    .iter()
                    .cloned()
                    .zip(bounds.iter().map(|bounds| bounds.top() - content_top))
                    .collect(),
                max: scroll.max_offset().height,
            };
            if state
                .last_layout_offset
                .is_some_and(|last| last != offset_before_layout)
            {
                state.restore = None;
            }
            let position = state.restore.take().or_else(|| {
                state
                    .geometry
                    .as_ref()
                    .filter(|previous| **previous != geometry)
                    .map(|previous| previous.position(offset_before_layout))
            });
            let adjustment = position.and_then(|position| {
                let target = geometry.offset(position.clone());
                ((target - current).abs() > px(0.5)).then_some((position, target))
            });
            state.geometry = Some(geometry);
            state.last_layout_offset = Some(current);
            state.revision = state.revision.wrapping_add(1);
            if let Some((position, target)) = adjustment {
                // Another layout can run before the scheduled frame. Keep the anchor
                // until the newest layout applies it or a scroll supersedes it.
                state.restore = Some(position);
                let revision = state.revision;
                let scroll = scroll.clone();
                let viewport = Rc::clone(&viewport);
                window.defer(cx, move |window, _| {
                    let mut state = viewport.borrow_mut();
                    if state.revision == revision {
                        state.restore = None;
                        if scroll.offset().y == current {
                            scroll.set_offset(point(scroll.offset().x, target));
                            state.last_layout_offset = Some(target);
                            window.refresh();
                        }
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_survive_new_rows_and_fall_back_to_clamped_pixels() {
        let initial = Geometry {
            rows: vec![("first".into(), px(28.0)), ("anchor".into(), px(100.0))],
            max: px(400.0),
        };
        let anchor = initial.position(px(-120.0));
        let updated = Geometry {
            rows: vec![("anchor".into(), px(170.0))],
            max: px(500.0),
        };
        assert_eq!(updated.offset(anchor.clone()), px(-190.0));
        assert_eq!(
            Geometry {
                rows: vec![],
                max: px(80.0)
            }
            .offset(anchor),
            px(-80.0)
        );
        assert_eq!(updated.offset(initial.position(px(-385.0))), px(-500.0));
    }

    #[test]
    fn expansion_is_scoped_and_explicit_choices_override_defaults() {
        let mut reading = Reading::default();
        reading.set_open(Some(0), "tool/read".into(), true);
        reading.set_open(Some(1), "activity/read".into(), false);
        assert!(reading.is_open(Some(0), "tool/read", false));
        assert!(!reading.is_open(Some(1), "tool/read", false));
        assert!(!reading.is_open(Some(1), "activity/read", true));
        assert!(reading.is_open(Some(0), "activity/read", true));
    }
}
