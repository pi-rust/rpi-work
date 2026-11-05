//! Animation-only estimate of live output cadence, never a billing/token metric.
//! Full messages and history are intentionally ignored: compact deltas carry no
//! reliable per-token count, so only their size contributes to this visual hint.
use dioxus::prelude::*;
use std::collections::{BTreeMap, VecDeque};
use web_time::Instant;
use work_client::{SessionEvent, WireEvent};
use work_core::ApplyResult;

const WINDOW_MS: f64 = 2_000.0;
const MAX_SCOPES: usize = 16;
const MAX_BUCKETS: usize = 16;

#[derive(Default)]
struct Session {
    cursor: Option<u64>,
    buckets: VecDeque<(f64, f64)>,
    rate: f64,
    touched: f64,
}

#[derive(Default)]
struct Store {
    sessions: BTreeMap<String, Session>,
}

fn valid_time(now: f64) -> bool {
    now.is_finite() && now >= 0.0
}

impl Store {
    fn observe_live(&mut self, scope: &str, event: &SessionEvent, now: f64) {
        // The connection knows the snapshot cursor; this store intentionally
        // sees live output only and cannot infer replay from its own cursor.
        if !matches!(
            event.applied,
            ApplyResult::IgnoredStale | ApplyResult::IgnoredDuplicate
        ) {
            self.observe(scope, &event.wire, now);
        }
    }

    fn observe(&mut self, scope: &str, event: &WireEvent, now: f64) {
        if !valid_time(now) || scope.is_empty() {
            return;
        }
        let reset = matches!(
            event.event_type.as_str(),
            "agent_start"
                | "message_start"
                | "message_end"
                | "tool_execution_start"
                | "ui_dialog_request"
                | "agent_settled"
                | "connection_reconnecting"
                | "connection_online"
                | "resync_required"
        );
        let data = &event.data;
        let kind = data["eventType"]
            .as_str()
            .or_else(|| data["assistantMessageEvent"]["type"].as_str());
        let delta = (event.event_type == "message_update"
            && matches!(
                kind,
                Some("text_delta" | "thinking_delta" | "toolcall_delta" | "tool_call_delta")
            )
            && data["message"]["role"]
                .as_str()
                .or_else(|| data["message"]["kind"].as_str())
                .is_none_or(|role| role == "assistant"))
        .then(|| {
            data["delta"]
                .as_str()
                .or_else(|| data["assistantMessageEvent"]["delta"].as_str())
        })
        .flatten()
        .filter(|delta| !delta.is_empty());
        if !reset && delta.is_none() {
            return;
        }
        if !self.sessions.contains_key(scope) && self.sessions.len() >= MAX_SCOPES {
            let oldest = self
                .sessions
                .iter()
                .min_by(|a, b| a.1.touched.total_cmp(&b.1.touched))
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.sessions.remove(&oldest);
            }
        }
        let session = self.sessions.entry(scope.into()).or_default();
        if now < session.touched
            || event
                .seq
                .is_some_and(|seq| session.cursor.is_some_and(|last| seq <= last))
        {
            return;
        }
        session.cursor = event.seq.or(session.cursor);
        session.touched = now;
        if reset {
            session.buckets.clear();
            session.rate = 1.0;
        }
        if let Some(delta) = delta {
            // Rough output units: ASCII ~4 chars/token; other scripts ~1.
            // Do not retain any text or count chunks as individual tokens.
            let units: f64 = delta
                .chars()
                .map(|ch| if ch.is_ascii() { 0.25 } else { 1.0 })
                .sum();
            let bucket = (now / 250.0).floor() * 250.0;
            // Quantized bucket times are monotonic, so >= means this bucket.
            if let Some((_, total)) = session.buckets.back_mut().filter(|(at, _)| *at >= bucket) {
                *total += units;
            } else {
                session.buckets.push_back((bucket, units));
            }
            while session.buckets.len() > MAX_BUCKETS {
                session.buckets.pop_front();
            }
        }
    }

    fn sample(&mut self, now: f64) -> BTreeMap<String, f64> {
        if !valid_time(now) {
            return BTreeMap::new();
        }
        self.sessions
            .retain(|_, session| now - session.touched < 60_000.0);
        let mut rates = BTreeMap::new();
        for (scope, session) in &mut self.sessions {
            while session
                .buckets
                .front()
                .is_some_and(|(at, _)| now - at >= WINDOW_MS)
            {
                session.buckets.pop_front();
            }
            let units: f64 = session.buckets.iter().map(|(_, units)| units).sum();
            let target = (1.0 + units / (WINDOW_MS / 1_000.0) / 40.0).clamp(1.0, 3.2);
            session.rate = session.rate.max(1.0) + (target - session.rate.max(1.0)) * 0.35;
            // Quantize only the published rate, not the smoother (which must
            // converge all the way back to 1 instead of sticking at 1.05).
            let rate = (session.rate * 20.0).round() / 20.0;
            if rate > 1.0 {
                rates.insert(scope.clone(), rate);
            }
        }
        rates
    }
}

#[derive(Clone, Copy)]
pub struct Feed {
    store: Signal<Store>,
    origin: Instant,
}

impl Feed {
    pub fn observe(mut self, scope: &str, event: &SessionEvent) {
        self.store.write().observe_live(
            scope,
            event,
            self.origin.elapsed().as_secs_f64() * 1_000.0,
        );
    }
}

#[derive(Clone, Copy)]
struct MotionContext {
    rates: Signal<BTreeMap<String, f64>>,
    scope: Memo<String>,
}

/// One small poller for the whole workspace, not one per spinner. Consumers
/// subscribe to rates only; neither the workspace nor transcript reads them.
pub fn use_output_motion(scope: Memo<String>) -> Feed {
    let store = use_signal(Store::default);
    let origin = use_hook(Instant::now);
    let mut rates = use_signal(BTreeMap::new);
    use_context_provider(|| MotionContext { rates, scope });
    use_future(move || async move {
        let mut store = store;
        loop {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(250).await;
            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let next = store
                .write()
                .sample(origin.elapsed().as_secs_f64() * 1_000.0);
            if *rates.peek() != next {
                rates.set(next);
            }
        }
    });
    Feed { store, origin }
}

pub fn use_rate(scope: Option<String>) -> f64 {
    let context = use_hook(try_consume_context::<MotionContext>);
    let Some(context) = context else { return 1.0 };
    let scope = scope.unwrap_or_else(|| context.scope.read().clone());
    context.rates.read().get(&scope).copied().unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(seq: u64, kind: &str, data: serde_json::Value) -> WireEvent {
        WireEvent {
            id: None,
            seq: Some(seq),
            event_type: kind.into(),
            data,
        }
    }
    fn delta(seq: u64, text: &str) -> WireEvent {
        event(
            seq,
            "message_update",
            json!({"eventType":"text_delta","delta":text}),
        )
    }
    fn rate(store: &mut Store, scope: &str, now: f64) -> f64 {
        store.sample(now).get(scope).copied().unwrap_or(1.0)
    }

    #[test]
    fn faster_output_is_faster_and_chunk_boundaries_do_not_matter() {
        let mut store = Store::default();
        store.observe("slow", &delta(1, &"x".repeat(40)), 0.0);
        store.observe("fast", &delta(1, &"x".repeat(800)), 0.0);
        store.observe("chunked", &delta(1, &"x".repeat(400)), 0.0);
        store.observe("chunked", &delta(2, &"x".repeat(400)), 100.0);
        let rates = store.sample(250.0);
        assert!(rates["fast"] > rates["slow"]);
        assert_eq!(rates["fast"], rates["chunked"]);
        for tick in 2..=30 {
            store.sample(f64::from(tick) * 250.0);
        }
        assert!(
            store.sample(8_000.0).is_empty(),
            "silent output must return to baseline"
        );
    }

    #[test]
    fn replay_snapshots_tools_and_other_sessions_do_not_inflate_output() {
        let mut store = Store::default();
        let first = delta(10, &"x".repeat(400));
        store.observe("node:a", &first, 0.0);
        store.observe("node:a", &first, 50.0);
        store.observe("node:a", &delta(9, &"x".repeat(400)), 100.0);
        store.observe(
            "node:b",
            &event(
                11,
                "message_update",
                json!({"message":{"role":"assistant","content":"snapshot"}}),
            ),
            100.0,
        );
        store.observe(
            "node:b",
            &event(
                12,
                "message_update",
                json!({"message":{"kind":"user"},"eventType":"text_delta","delta":"ignore"}),
            ),
            100.0,
        );
        store.observe(
            "node:b",
            &event(13, "tool_execution_end", json!({"output":"huge result"})),
            100.0,
        );
        assert_eq!(store.sessions["node:a"].buckets[0].1, 100.0);
        assert_eq!(rate(&mut store, "node:b", 250.0), 1.0);
        assert!(rate(&mut store, "node:a", 500.0) > 1.0);
    }

    #[test]
    fn reasoning_and_tool_arguments_count_but_boundaries_reset() {
        for kind in ["thinking_delta", "toolcall_delta", "tool_call_delta"] {
            for boundary in [
                "agent_start",
                "message_start",
                "message_end",
                "tool_execution_start",
                "ui_dialog_request",
                "agent_settled",
                "connection_reconnecting",
                "connection_online",
            ] {
                let mut store = Store::default();
                store.observe(
                    "a",
                    &event(
                        1,
                        "message_update",
                        json!({"assistantMessageEvent":{"type":kind,"delta":"输出".repeat(100)}}),
                    ),
                    0.0,
                );
                assert!(rate(&mut store, "a", 250.0) > 1.0);
                store.observe("a", &event(2, boundary, json!({})), 300.0);
                assert_eq!(rate(&mut store, "a", 500.0), 1.0, "{kind}/{boundary}");
                store.observe("a", &delta(1, "old replay"), 600.0);
                assert_eq!(rate(&mut store, "a", 750.0), 1.0);
            }
        }
    }

    #[test]
    fn connection_snapshot_cursor_rejects_replay_even_without_a_motion_watermark() {
        let mut store = Store::default();
        store.observe("a", &delta(10, "live output"), 0.0);
        // A reconnect snapshot advanced the connection cursor to 100, but its
        // history was never sent to this animation-only observer.
        let stale = SessionEvent {
            wire: delta(50, &"old replay ".repeat(100)),
            applied: ApplyResult::IgnoredStale,
        };
        store.observe_live("a", &stale, 500.0);
        store.observe_live("newly-opened", &stale, 500.0);
        assert_eq!(store.sessions["a"].cursor, Some(10));
        assert_eq!(store.sessions["a"].buckets.len(), 1);
        assert!(!store.sessions.contains_key("newly-opened"));
        let duplicate = SessionEvent {
            wire: WireEvent {
                seq: None,
                ..delta(50, "duplicated output")
            },
            applied: ApplyResult::IgnoredDuplicate,
        };
        store.observe_live("newly-opened", &duplicate, 550.0);
        assert!(!store.sessions.contains_key("newly-opened"));
        let boundary = SessionEvent {
            wire: event(51, "message_end", json!({})),
            applied: ApplyResult::IgnoredStale,
        };
        store.observe_live("a", &boundary, 600.0);
        assert!(
            !store.sessions["a"].buckets.is_empty(),
            "stale boundaries cannot reset live output"
        );
    }

    #[test]
    fn bounded_storage_and_bad_clock_are_safe() {
        let mut store = Store::default();
        for i in 0..100 {
            store.observe(&format!("scope:{i}"), &delta(1, "output"), f64::from(i));
        }
        assert_eq!(store.sessions.len(), MAX_SCOPES);
        for i in 0..200_u32 {
            store.observe(
                "bounded",
                &delta(u64::from(i) + 1, "output"),
                f64::from(i) * 250.0,
            );
        }
        assert!(store.sessions["bounded"].buckets.len() <= MAX_BUCKETS);
        for now in [f64::NAN, f64::INFINITY, -1.0] {
            store.observe("bad", &delta(1, "output"), now);
            assert!(store.sample(now).is_empty());
        }
        assert!(!store.sessions.contains_key("bad"));
        assert!(store.sample(200_000.0).is_empty());
        assert!(store.sessions.is_empty());
    }
}
