//! Quiet, memory-only views of client receive timing; never mutates `TraceStore`.
use crate::features::observability::traces::{self, ToolTiming, TraceRun, TraceStore};
use dioxus::prelude::*;

pub const CSS: &str = include_str!("turn_timing.css");
const MEASUREMENT: &str = "客户端接收计时";

/// Provided by the workspace, but deliberately optional in isolated tool harnesses.
#[derive(Clone, Copy)]
pub struct TimingContext {
    pub store: Signal<TraceStore>,
    pub scope: Memo<String>,
}

fn recorded(ms: Option<f64>) -> Option<f64> {
    ms.filter(|value| value.is_finite() && *value >= 0.0)
}

fn elapsed_label(ms: Option<f64>) -> String {
    match recorded(ms) {
        Some(ms) => traces::duration(Some(ms)),
        None => "— 未记录".into(),
    }
}

// This is a capability probe, NOT a displayed timestamp: the public elapsed API
// rejects missing starts. It lets us avoid even starting a poller for those runs.
fn can_tick(run: &TraceRun) -> bool {
    run.source == "live"
        && run.is_open()
        && !run.clock_failed
        && recorded(run.elapsed_at(Some(f64::MAX))).is_some()
}

/// One local clock per observed view. The key changes on scope/run/tool changes;
/// cancelling the task also drops an in-flight eval or platform sleep. Neither
/// the parent nor the trace store subscribes to this signal.
fn use_local_now(key: Option<String>) -> Option<f64> {
    let mut sample = use_signal(|| None::<(String, f64)>);
    let mut task = use_signal(|| None::<dioxus::core::Task>);
    use_effect(use_reactive((&key,), move |(key,)| {
        if let Some(previous) = task.peek().as_ref().copied() {
            previous.cancel();
        }
        task.set(None);
        sample.set(None);
        let Some(key) = key else { return };
        task.set(Some(spawn(async move {
            loop {
                let mut eval = document::eval(
                    "try { dioxus.send(performance.now()); } catch (_) { dioxus.send(null); }",
                );
                let now = eval
                    .recv::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|value| value.as_f64());
                let Some(now) = recorded(now) else {
                    sample.set(None);
                    break;
                };
                sample.set(Some((key.clone(), now)));
                #[cfg(target_arch = "wasm32")]
                gloo_timers::future::TimeoutFuture::new(1_000).await;
                #[cfg(not(target_arch = "wasm32"))]
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        })));
    }));
    use_drop(move || {
        if let Some(previous) = task.write().take() {
            previous.cancel();
        }
    });
    let expected = key?;
    sample
        .read()
        .as_ref()
        .filter(|(sample_key, _)| *sample_key == expected)
        .map(|(_, now)| *now)
}

#[component]
pub fn ToolElapsed(id: String, timing: Option<ToolTiming>) -> Element {
    let context = use_hook(try_consume_context::<TimingContext>);
    let scope = context.map(|context| context.scope.read().clone());
    // History only supplies completed, validated node times. Never restart a
    // timer from a persisted running sample or from message timestamps.
    let snapshot =
        timing.filter(|timing| !timing.running && recorded(Some(timing.elapsed_ms)).is_some());
    let key = context.and_then(|context| {
        if snapshot.is_some() {
            return None;
        }
        let scope = scope.as_deref()?;
        let store = context.store.read();
        let (run, span) = store.tool_span(scope, &id)?;
        (run.source == "live"
            && run.is_open()
            && span.is_open()
            && (span.node_can_tick()
                || (!run.clock_failed && recorded(span.elapsed_at(Some(f64::MAX))).is_some())))
        .then(|| format!("{}:{scope}:{}:{}", scope.len(), run.id, span.id))
    });
    // Hooks remain unconditional, including the no-provider harness fallback.
    let now = use_local_now(key.clone());
    let observed = context.and_then(|context| {
        let store = context.store.read();
        let (run, span) = store.tool_span(scope.as_deref()?, &id)?;
        if run.source != "live" {
            return None;
        }
        let now = if key.is_some() && span.is_open() {
            now
        } else {
            None
        };
        if let Some(elapsed) = span.node_elapsed_at(now) {
            return Some((elapsed, "节点执行耗时"));
        }
        if run.clock_failed {
            return None;
        }
        // A settled span never gets `now` as a substitute for a missing end.
        recorded(span.elapsed_at(now)).map(|elapsed| (elapsed, MEASUREMENT))
    });
    let measured = snapshot
        .map(|timing| (timing.elapsed_ms, "节点执行耗时"))
        .or(observed);
    let elapsed = measured.map(|(elapsed, _)| elapsed);
    let text = traces::duration(elapsed);
    let description = match measured {
        Some((_, source)) => format!("{source} · {text}"),
        None => format!("{MEASUREMENT} · 未记录"),
    };
    rsx! {
        span { class: "tool-elapsed", role: "img", title: "{description}", aria_label: "{description}",
            crate::shared::components::Icon { name: "clock" }
            span { "{text}" }
        }
    }
}

/// Diagnostic timing is separate from the user-input navigation rail and mounts on demand.
#[component]
pub fn TimingInspector(scope: String, store: Signal<TraceStore>) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        section { class: "inspector-card timing-inspector",
            button { class: "inspector-small-button", r#type: "button", aria_expanded: *open.read(),
                onclick: move |_| { let next = !*open.peek(); open.set(next); },
                "回合耗时"
            }
            if *open.read() { TurnTimeline { scope, store } }
        }
    }
}

/// Run identity and snapshots flow downward. Only individual run views tick,
/// so a one-second clock never invalidates this list or the conversation input.
#[component]
pub fn TurnTimeline(scope: String, store: Signal<TraceStore>) -> Element {
    let state = store.read();
    let session = state.scopes.get(&scope);
    let notice = session.and_then(|session| session.notice.clone());
    let mut number = 0;
    let runs: Vec<_> = session
        .into_iter()
        .flat_map(|session| &session.runs)
        .map(|run| {
            let turn = if run.source == "history" {
                None
            } else {
                number += 1;
                Some(number)
            };
            (run.id.clone(), turn)
        })
        .collect();
    rsx! {
        section { class: "turn-timeline", aria_label: "回合耗时记录",
            div { class: "turn-timing-heading",
                span { "回合耗时" }
                span { title: "客户端接收计时，包含等待、传输与调度；不是服务端纯执行耗时。", "客户端计时" }
            }
            if let Some(notice) = notice {
                p { class: "turn-timing-notice", role: "status", "{notice}" }
            }
            if runs.is_empty() {
                p { class: "turn-timing-empty", "— 未记录" }
            }
            for (run_id, number) in runs {
                TurnTimingRun { key: "{scope}:{run_id}", scope: scope.clone(), store, run_id, number }
            }
        }
    }
}

#[component]
fn TurnTimingRun(
    scope: String,
    store: Signal<TraceStore>,
    run_id: String,
    number: Option<usize>,
) -> Element {
    // Preserve native details/summary keyboard behavior, but avoid thousands of
    // hidden step nodes in long sessions until a round is explicitly expanded.
    let mut revealed = use_signal(|| false);
    let state = store.read();
    let run = state
        .scopes
        .get(&scope)
        .and_then(|session| session.runs.iter().find(|run| run.id == run_id));
    let key = run
        .filter(|run| can_tick(run))
        .map(|run| format!("{}:{scope}:{}", scope.len(), run.id));
    let now = use_local_now(key.clone());
    let Some(run) = run else { return rsx! {} };
    let history = run.source == "history";
    let now = if key.is_some() && run.is_open() {
        now
    } else {
        None
    };
    let measured = run.source == "live" && !run.clock_failed;
    let elapsed = elapsed_label(if measured { run.elapsed_at(now) } else { None });
    let title = match number {
        Some(number) => format!("回合 {number}"),
        None => "历史记录".into(),
    };
    let state = traces::status_label(&run.status);
    rsx! {
        details { class: "turn-timing-run", "data-run-id": "{run.id}", "data-status": "{run.status}",
            summary { class: "turn-timing-summary", onclick: move |_| revealed.set(true),
                span { class: "turn-timing-title", "{title}" }
                span { class: "turn-timing-state", " · {state} · 耗时 " }
                span { class: "turn-timing-duration", title: "客户端接收计时", "{elapsed}" }
            }
            if *revealed.read() {
            div { class: "turn-timing-body",
                p { class: "turn-timing-note", "客户端接收计时，包含等待、传输与调度，并行步骤不相加。仅保留当前客户端内最近的观测记录，未记录的时间显示 —。" }
                if history {
                    p { class: "turn-timing-note", "历史快照，仅表示记录顺序；不是一个真实回合，耗时未记录。" }
                } else {
                    p { class: "turn-timing-run-name", "{run.name}" }
                }
                ol { class: "turn-timing-spans", aria_label: "按客户端接收顺序排列的阶段",
                    for span in &run.spans {
                        {
                            let offset = if measured { recorded(run.offset(span)) } else { None };
                            let offset = match offset {
                                Some(ms) => format!("+{}", traces::duration(Some(ms))),
                                None => "— 未记录".into(),
                            };
                            let duration = elapsed_label(if measured && span.id == "root" {
                                run.elapsed_at(now)
                            } else if measured {
                                span.elapsed_at(if span.is_open() { now } else { None })
                            } else {
                                None
                            });
                            let kind = traces::kind_label(&span.kind);
                            let status = traces::status_label(&span.status);
                            // Include the root once, explicitly as the whole turn.
                            let name = if span.id == "root" { "整个回合" } else { &span.name };
                            rsx! {
                                li { class: "turn-timing-span", key: "{span.id}", "data-span-id": "{span.id}",
                                    span { class: "turn-timing-offset", title: "相对回合开始的客户端接收偏移", "{offset}" }
                                    span { class: "turn-timing-span-name",
                                        span { class: "turn-timing-kind", "{kind} · " }
                                        "{name}"
                                    }
                                    span { class: "turn-timing-span-status", "{status}" }
                                    span { class: "turn-timing-span-duration", title: "客户端接收计时", "{duration}" }
                                }
                            }
                        }
                    }
                }
            }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{elapsed_label, recorded};

    #[test]
    fn missing_and_invalid_measurements_stay_unknown() {
        for value in [None, Some(-1.0), Some(f64::NAN), Some(f64::INFINITY)] {
            assert_eq!(recorded(value), None);
            assert_eq!(elapsed_label(value), "— 未记录");
        }
        assert_eq!(elapsed_label(Some(0.0)), "<1ms");
        assert_eq!(elapsed_label(Some(1250.0)), "1.25s");
    }
}
