//! Reading position and tool expansion for the native transcript.

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

use gpui::{App, Bounds, Context, Pixels, ScrollHandle, Task, WeakEntity, Window, point, px};
use serde_json::{Value, json};

use super::{Shell, session_storage};

#[derive(Clone, Debug, PartialEq)]
enum Position {
    Bottom,
    Offset(Pixels),
    Anchor {
        key: String,
        durable: bool,
        within_row: Pixels,
        offset: Pixels,
    },
}

impl Position {
    fn value(&self) -> Value {
        match self {
            Self::Bottom => json!({ "bottom": true }),
            Self::Offset(offset)
            | Self::Anchor {
                offset,
                durable: false,
                ..
            } => {
                json!({ "offset": f32::from(*offset) })
            }
            Self::Anchor {
                key,
                durable: true,
                within_row,
                offset,
            } => json!({
                "key": key,
                "withinRow": f32::from(*within_row),
                "offset": f32::from(*offset),
            }),
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        if value["bottom"].as_bool() == Some(true) {
            return Some(Self::Bottom);
        }
        let pixels = |name: &str| {
            let value = value[name].as_f64()? as f32;
            value.is_finite().then(|| px(value))
        };
        let offset = pixels("offset")?;
        match (value["key"].as_str(), pixels("withinRow")) {
            (Some(key), Some(within_row)) => Some(Self::Anchor {
                key: key.into(),
                durable: true,
                within_row,
                offset,
            }),
            _ => Some(Self::Offset(offset)),
        }
    }
}

#[derive(Default, PartialEq)]
struct Geometry {
    rows: Vec<(String, bool, Pixels)>,
    max: Pixels,
}

impl Geometry {
    fn has_later_content(&self, offset: Pixels) -> bool {
        self.max + offset > px(1.0)
    }

    fn position(&self, offset: Pixels) -> Position {
        if !self.has_later_content(offset) {
            return Position::Bottom;
        }
        let index = self.rows.partition_point(|(_, _, top)| *top <= -offset);
        self.rows.get(index.saturating_sub(1)).map_or(
            Position::Offset(offset),
            |(key, durable, top)| Position::Anchor {
                key: key.clone(),
                durable: *durable,
                within_row: *top + offset,
                offset,
            },
        )
    }

    fn offset(&self, position: Position) -> Pixels {
        let offset = match position {
            Position::Bottom => -self.max,
            Position::Offset(offset) => offset,
            Position::Anchor {
                key,
                durable,
                within_row,
                offset,
            } => self
                .rows
                .iter()
                .find(|(candidate, stable, _)| candidate == &key && (!durable || *stable))
                .map_or(offset, |(_, _, top)| within_row - *top),
        };
        offset.clamp(-self.max, px(0.0))
    }
}

#[derive(Default)]
struct Viewport {
    geometry: Option<Geometry>,
    restore: Option<Position>,
    last_layout_offset: Option<Pixels>,
    later_content: bool,
    revision: u64,
}

#[derive(Default)]
pub(super) struct Reading {
    viewport: Rc<RefCell<Viewport>>,
    positions: HashMap<usize, Position>,
    open: HashMap<usize, HashMap<String, bool>>,
    checkpoints: HashMap<usize, Checkpoint>,
    save_task: Option<Task<()>>,
}

struct Checkpoint {
    path: PathBuf,
    saved: Value,
    failed: bool,
}

impl Reading {
    fn position(&self, scroll: &ScrollHandle) -> Option<Position> {
        let viewport = self.viewport.borrow();
        viewport.restore.clone().or_else(|| {
            viewport
                .geometry
                .as_ref()
                .map(|geometry| geometry.position(scroll.offset().y))
        })
    }

    pub(super) fn load(&mut self, session: usize, path: PathBuf) {
        if self.checkpoints.contains_key(&session) {
            return;
        }
        let saved = match session_storage::read(&path) {
            Ok(value) => value.unwrap_or_default(),
            Err(error) => {
                eprintln!("Could not restore conversation reading state: {error}");
                Value::Null
            }
        };
        if let Some(position) = Position::from_value(&saved["position"]) {
            self.positions.insert(session, position);
        }
        if let Some(open) = saved.get("open").and_then(Value::as_object) {
            self.open.insert(
                session,
                open.iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_bool()?)))
                    .collect(),
            );
        }
        self.checkpoints.insert(
            session,
            Checkpoint {
                path,
                saved,
                failed: false,
            },
        );
    }

    pub(super) fn save(&mut self, session: Option<usize>, scroll: &ScrollHandle) {
        let Some((session, position)) = session.zip(self.position(scroll)) else {
            return;
        };
        let Some(checkpoint) = self.checkpoints.get_mut(&session) else {
            return;
        };
        let open = self.open.get(&session);
        if checkpoint.saved.is_null()
            && position == Position::Bottom
            && open.is_none_or(HashMap::is_empty)
        {
            return;
        }
        let record = json!({
            "position": position.value(),
            "open": open.map_or_else(|| json!({}), |open| json!(open)),
        });
        if checkpoint.saved == record {
            return;
        }
        match session_storage::write(&checkpoint.path, &record) {
            Ok(()) => {
                checkpoint.saved = record;
                checkpoint.failed = false;
            }
            Err(error) => {
                if !checkpoint.failed {
                    eprintln!("Could not save conversation reading state: {error}");
                }
                checkpoint.failed = true;
            }
        }
    }

    pub(super) fn switch(
        &mut self,
        previous: Option<usize>,
        next: Option<usize>,
        scroll: &ScrollHandle,
        start_at_bottom: bool,
    ) {
        self.save_task.take();
        self.save(previous, scroll);
        if let Some((previous, position)) = previous.zip(self.position(scroll)) {
            self.positions.insert(previous, position);
        }
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

    pub(super) fn later_content(&self) -> bool {
        self.viewport.borrow().later_content
    }

    pub(super) fn scroll_to(&self, scroll: &ScrollHandle, offset: Pixels) {
        let mut state = self.viewport.borrow_mut();
        state.restore = None;
        state.revision = state.revision.wrapping_add(1);
        scroll.set_offset(point(scroll.offset().x, offset));
    }

    pub(super) fn layout(
        &self,
        scroll: ScrollHandle,
        keys: Vec<(String, bool)>,
        view: WeakEntity<Shell>,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let viewport = Rc::clone(&self.viewport);
        let offset_before_layout = scroll.offset().y;
        if !keys.is_empty() {
            let state = viewport.borrow();
            let restore = state.restore.as_ref().filter(|_| {
                state
                    .last_layout_offset
                    .is_none_or(|last| last == offset_before_layout)
            });
            let following = restore.map_or_else(
                || {
                    state
                        .geometry
                        .as_ref()
                        .is_none_or(|geometry| !geometry.has_later_content(offset_before_layout))
                },
                |position| *position == Position::Bottom,
            );
            if following {
                scroll.scroll_to_bottom();
            }
        }
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
                    .zip(&bounds)
                    .map(|((key, durable), bounds)| {
                        (key.clone(), *durable, bounds.top() - content_top)
                    })
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
            let target = adjustment.as_ref().map_or(current, |(_, target)| *target);
            let later_content = geometry.has_later_content(target);
            let visibility_changed = state.later_content != later_content;
            let position_changed = state.last_layout_offset != Some(target);
            state.later_content = later_content;
            state.geometry = Some(geometry);
            state.last_layout_offset = Some(current);
            state.revision = state.revision.wrapping_add(1);
            if visibility_changed || position_changed {
                let view = view.clone();
                let viewport = Rc::clone(&viewport);
                window.defer(cx, move |_, cx| {
                    let _ = view.update(cx, |shell, cx| {
                        if !Rc::ptr_eq(&shell.reading.viewport, &viewport) {
                            return;
                        }
                        if position_changed {
                            shell.schedule_reading_save(cx);
                        }
                        if visibility_changed {
                            cx.notify();
                        }
                    });
                });
            }
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

impl Shell {
    pub(super) fn load_reading_state(&mut self, index: usize) {
        let Some(node) = session_storage::node_scope(&self.snapshot) else {
            return;
        };
        let Some(directory) = session_storage::directory("gpui-reading") else {
            return;
        };
        let session = &self.snapshot.sessions[index];
        let path = session_storage::file(&directory, &node, &session.project_id, Some(&session.id));
        self.reading.load(index, path);
    }

    pub(super) fn save_reading_state(&mut self) {
        self.reading.save(self.session, &self.transcript_scroll);
    }

    pub(super) fn schedule_reading_save(&mut self, cx: &mut Context<Self>) {
        if self.reading.save_task.is_some()
            || !self
                .session
                .is_some_and(|session| self.reading.checkpoints.contains_key(&session))
        {
            return;
        }
        let timer = cx.background_executor().timer(Duration::from_millis(300));
        self.reading.save_task = Some(cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |shell, _| {
                shell.reading.save_task.take();
                shell.save_reading_state();
            });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_survive_new_rows_and_fall_back_to_clamped_pixels() {
        let initial = Geometry {
            rows: vec![
                ("first".into(), true, px(28.0)),
                ("anchor".into(), true, px(100.0)),
            ],
            max: px(400.0),
        };
        let anchor = initial.position(px(-120.0));
        let updated = Geometry {
            rows: vec![("anchor".into(), true, px(170.0))],
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
        assert_eq!(updated.offset(initial.position(px(-400.0))), px(-500.0));
    }

    #[test]
    fn scrolling_even_a_little_above_bottom_keeps_the_reading_anchor() {
        let before = Geometry {
            rows: vec![("reply".into(), true, px(28.0))],
            max: px(400.0),
        };
        let after = Geometry {
            rows: vec![("reply".into(), true, px(28.0))],
            max: px(450.0),
        };
        assert!(before.has_later_content(px(-390.0)));
        assert_eq!(after.offset(before.position(px(-390.0))), px(-390.0));
        assert!(!before.has_later_content(px(-399.5)));
        assert_eq!(after.offset(before.position(px(-399.5))), px(-450.0));
        assert!(!Geometry::default().has_later_content(px(0.0)));
    }

    #[test]
    fn local_row_keys_are_saved_as_pixel_offsets() {
        let geometry = Geometry {
            rows: vec![("row/1".into(), false, px(28.0))],
            max: px(400.0),
        };
        let position = geometry.position(px(-40.0));
        assert_eq!(
            Position::from_value(&position.value()),
            Some(Position::Offset(px(-40.0)))
        );
    }

    #[test]
    fn restores_an_anchor_and_explicit_tool_choices_under_a_new_window_index() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reading.json");
        let scroll = ScrollHandle::new();
        let mut original = Reading::default();
        original.load(0, path.clone());
        original.switch(None, Some(0), &scroll, true);
        let anchor = Position::Anchor {
            key: "text/reply-1".into(),
            durable: true,
            within_row: px(-12.0),
            offset: px(-240.0),
        };
        original.viewport.borrow_mut().restore = Some(anchor.clone());
        original.set_open(Some(0), "tool/read".into(), true);
        original.set_open(Some(0), "activity/tasks".into(), false);
        original.save(Some(0), &scroll);

        let mut reopened = Reading::default();
        reopened.load(1, path);
        reopened.switch(None, Some(1), &scroll, true);
        assert_eq!(reopened.position(&scroll), Some(anchor));
        assert!(reopened.is_open(Some(1), "tool/read", false));
        assert!(!reopened.is_open(Some(1), "activity/tasks", true));
        assert!(!reopened.is_open(Some(0), "tool/read", false));
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
