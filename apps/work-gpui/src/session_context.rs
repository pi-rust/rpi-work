//! Read-only facts about the selected session.

use std::time::{Duration, Instant};

use gpui::{
    Context, FocusHandle, FontWeight, ScrollHandle, Task, Window, div, point, prelude::*, px,
    relative,
};
use serde_json::Value;
use work_client::RuntimeState;
use work_viewmodel::observability::metrics::{Context as Usage, Metrics, grouped, percent};

use super::{Shell, composer_config::thinking_label};

#[derive(Default)]
pub(super) struct RunTiming {
    start: Option<Instant>,
    elapsed: Option<Duration>,
}

impl RunTiming {
    pub(super) fn observe(&mut self, event: &str, now: Instant) {
        match event {
            "operation_accepted" => self.elapsed = None,
            "agent_start" => {
                self.start.get_or_insert(now);
                self.elapsed = None;
            }
            "agent_settled" => self.elapsed = self.start.take().map(|start| now - start),
            "connection_reconnecting" | "resync_required" | "connection_online" => {
                *self = Self::default();
            }
            _ => {}
        }
    }

    pub(super) fn elapsed(&self) -> Option<String> {
        let duration = self.elapsed?;
        let seconds = duration.as_secs();
        Some(match seconds {
            0 => format!("{} ms", duration.as_millis().max(1)),
            1..60 => format!("{:.1} s", duration.as_secs_f64()),
            _ => format!("{} 分 {:02} 秒", seconds / 60, seconds % 60),
        })
    }
}

pub(super) struct Panel {
    pub(super) open: bool,
    close: FocusHandle,
    scroll: ScrollHandle,
    read: Option<Task<()>>,
    runtime: Option<RuntimeState>,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> Self {
        Self {
            open: false,
            close: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            read: None,
            runtime: None,
        }
    }

    pub(super) fn reset(&mut self) {
        self.open = false;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.read = None;
        self.runtime = None;
    }
}

fn loaded_usage(history: &[Value]) -> Option<Metrics> {
    let mut total = Metrics::default();
    let mut complete_input = true;
    for entry in history {
        let message = entry.get("message").unwrap_or(entry);
        if message["role"] != "assistant" {
            continue;
        }
        let usage = Metrics::turn(message.get("usage"));
        complete_input &= usage.input.is_some() && usage.cached.is_some();
        for (sum, value) in [
            (&mut total.input, usage.input),
            (&mut total.output, usage.output),
            (&mut total.cached, usage.cached),
            (&mut total.writes, usage.writes),
        ] {
            if let Some(value) = value {
                *sum = Some(sum.unwrap_or_default() + value);
            }
        }
    }
    if !has_token_usage(&total) {
        return None;
    }
    total.cache_percent = total
        .cached
        .zip(total.input)
        .filter(|(cached, input)| complete_input && *input > 0.0 && cached <= input)
        .map(|(cached, input)| cached / input * 100.0);
    Some(total)
}

fn has_token_usage(metrics: &Metrics) -> bool {
    [
        metrics.input,
        metrics.output,
        metrics.cached,
        metrics.writes,
    ]
    .into_iter()
    .flatten()
    .any(|value| value > 0.0)
}

impl Shell {
    pub(super) fn composer_usage(&self, runtime: Option<&RuntimeState>) -> (Usage, Metrics, bool) {
        if self.node.is_none() && self.session.is_some() && self.session == self.transcript_session
        {
            return (
                self.snapshot.context.clone(),
                self.snapshot.metrics.clone(),
                false,
            );
        }
        let context = Usage::reported(runtime.and_then(|state| state.context.as_ref()));
        let reported = Metrics::session(runtime.and_then(|state| state.stats.as_ref()));
        // Some rpi versions return zero cumulative stats alongside nonzero message usage.
        if !has_token_usage(&reported)
            && let Some(metrics) = self
                .frame
                .as_ref()
                .and_then(|frame| loaded_usage(&frame.history))
        {
            return (context, metrics, true);
        }
        (context, reported, false)
    }

    pub(super) fn open_session_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_none()
            || !self.navigation_keyboard_enabled()
            || self.composer_is_marked()
        {
            return;
        }
        self.close_resource_page();
        self.project_menu = None;
        self.close_session_filter_menu();
        self.composer_config.close_menu();
        self.browser.restore_composer_focus();
        self.input_dragging = false;
        self.session_context.open = true;
        self.session_context.close.focus(window);
        if let Some((node, frame)) = self.node.as_ref().zip(self.frame.as_ref())
            && frame.projection.connection == work_core::ConnectionState::Online
            && frame
                .access
                .as_ref()
                .is_some_and(|access| access.supports_command("get_state"))
        {
            let read = node.read_runtime_state(frame.session_id.clone());
            self.session_context.read = Some(cx.spawn(async move |this, cx| {
                let runtime = read.await.ok().and_then(Result::ok);
                let _ = this.update(cx, |shell, cx| {
                    shell.session_context.read = None;
                    shell.session_context.runtime = runtime;
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }

    fn close_session_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session_context.reset();
        self.focus_composer(window);
        cx.notify();
    }

    pub(super) fn session_context_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let missing = "—";
        let runtime = self.session_context.runtime.as_ref().or_else(|| {
            self.composer_config
                .observed
                .as_ref()
                .map(|observed| &observed.runtime)
        });
        let (usage, metrics, loaded) = self.composer_usage(runtime);
        let history = if self.node.is_none() && self.session == self.transcript_session {
            Some(self.snapshot.history.as_slice())
        } else {
            self.frame.as_ref().map(|frame| frame.history.as_slice())
        };
        let turns = history.map(|history| {
            history
                .iter()
                .filter(|message| message["role"] == "user")
                .count()
        });
        let model = runtime.and_then(|state| state.model_id.as_deref());
        let model = match (model, runtime.and_then(|state| state.provider.as_deref())) {
            (Some(model), Some(provider)) => format!("{provider} · {model}"),
            (Some(model), None) => model.to_owned(),
            _ => missing.into(),
        };
        let location = self.navigation_node_label();
        let context = (usage.tokens.is_some() || usage.window.is_some() || usage.percent.is_some())
            .then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .rounded(px(8.0))
                    .bg(palette.subtle)
                    .px(px(10.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(self.text("上下文窗口", 12.0, palette.text))
                            .when(usage.tokens.is_some() || usage.window.is_some(), |row| {
                                row.child(self.text(
                                    format!(
                                        "{} / {} Token",
                                        grouped(usage.tokens),
                                        grouped(usage.window)
                                    ),
                                    12.0,
                                    palette.muted,
                                ))
                            }),
                    )
                    .when_some(usage.percent, |card, used| {
                        card.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .h(px(4.0))
                                        .rounded_full()
                                        .overflow_hidden()
                                        .bg(palette.strong_line)
                                        .child(
                                            div()
                                                .h_full()
                                                .w(relative(used as f32 / 100.0))
                                                .bg(palette.muted),
                                        ),
                                )
                                .child(self.text(percent(Some(used)), 12.0, palette.text)),
                        )
                    })
            });
        let counters = [
            ("输入（含缓存）", metrics.input),
            ("输出", metrics.output),
            ("缓存读取", metrics.cached),
            ("缓存写入", metrics.writes),
        ];
        let token_usage =
            counters.iter().any(|(_, value)| value.is_some()) || metrics.cache_percent.is_some();
        let timing = self.run_timing.elapsed();
        let fields = [
            (
                "项目",
                self.project
                    .map_or(missing, |index| self.snapshot.projects[index].name.as_str())
                    .to_owned(),
            ),
            ("执行位置", location),
            ("模型", model),
            (
                "推理强度",
                runtime
                    .and_then(|state| state.thinking_level.as_deref())
                    .map_or(missing, thinking_label)
                    .to_owned(),
            ),
            (
                "消息轮次",
                turns.map_or_else(|| missing.into(), |count| count.to_string()),
            ),
            (
                "后续消息",
                self.queue
                    .pending_count()
                    .map_or_else(|| missing.into(), |count| count.to_string()),
            ),
            ("权限", "节点未报告".into()),
        ];
        div()
            .id("session-context-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000030))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .capture_key_down(
                cx.listener(|shell, event: &gpui::KeyDownEvent, window, cx| {
                    let modifiers = event.keystroke.modifiers;
                    match event.keystroke.key.as_str() {
                        "escape" if !modifiers.modified() => {
                            shell.close_session_context(window, cx)
                        }
                        "tab" if !modifiers.alt && !modifiers.control && !modifiers.platform => {
                            shell.session_context.close.focus(window)
                        }
                        key @ ("up" | "down" | "pageup" | "pagedown" | "home" | "end")
                            if !modifiers.modified() =>
                        {
                            let scroll = &shell.session_context.scroll;
                            let offset = scroll.offset();
                            let bottom = -scroll.max_offset().height;
                            let line = px(24.0);
                            let page = (scroll.bounds().size.height - line).max(line);
                            let y = match key {
                                "up" => offset.y + line,
                                "down" => offset.y - line,
                                "pageup" => offset.y + page,
                                "pagedown" => offset.y - page,
                                "home" => px(0.0),
                                _ => bottom,
                            };
                            scroll.set_offset(point(offset.x, y.clamp(bottom, px(0.0))));
                            window.refresh();
                            window.prevent_default();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("session-context-dialog")
                    .tab_group()
                    .w_full()
                    .max_w(px(450.0))
                    .max_h((window.viewport_size().height - px(24.0)).min(px(490.0)))
                    .flex()
                    .flex_col()
                    .p(px(15.0))
                    .rounded(px(14.0))
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .flex_none()
                            .mb_2()
                            .child(
                                self.text("会话上下文", 16.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                self.navigation_tooltip(
                                    self.icon_button("close-session-context", "x", 32.0),
                                    "关闭上下文",
                                )
                                .track_focus(&self.session_context.close)
                                .tab_index(0)
                                .focus(|button| button.bg(palette.hover))
                                .on_click(cx.listener(
                                    |shell, _, window, cx| shell.close_session_context(window, cx),
                                )),
                            ),
                    )
                    .child(
                        div()
                            .id("session-context-fields")
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.session_context.scroll)
                            .children(context)
                            .when(token_usage, |body| {
                                body.child(
                                    div()
                                        .mt(px(10.0))
                                        .pb(px(4.0))
                                        .flex()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            self.text("Token 用量", 12.0, palette.text)
                                                .font_weight(FontWeight::SEMIBOLD),
                                        )
                                        .child(self.text(
                                            if loaded {
                                                "已加载消息"
                                            } else {
                                                "会话累计"
                                            },
                                            11.0,
                                            palette.muted,
                                        )),
                                )
                                .children(
                                    counters
                                        .chunks(2)
                                        .filter(|pair| {
                                            pair.iter().any(|(_, value)| value.is_some())
                                        })
                                        .map(|pair| {
                                            div().flex().gap_3().children(pair.iter().map(
                                                |(label, value)| {
                                                    div().flex_1().min_w_0().when_some(
                                                        *value,
                                                        |item, value| {
                                                            item.child(self.context_fact(
                                                                label,
                                                                grouped(Some(value)),
                                                            ))
                                                        },
                                                    )
                                                },
                                            ))
                                        }),
                                )
                                .when_some(
                                    metrics.cache_percent,
                                    |body, ratio| {
                                        body.child(
                                            self.context_fact("缓存命中率", percent(Some(ratio))),
                                        )
                                    },
                                )
                            })
                            .when_some(timing, |body, elapsed| {
                                body.child(self.context_fact("最近一轮耗时（桌面观测）", elapsed))
                            })
                            .child(
                                self.text("会话信息", 12.0, palette.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .mt(px(10.0))
                                    .pb(px(4.0)),
                            )
                            .children(
                                fields
                                    .into_iter()
                                    .map(|(label, value)| self.context_fact(label, value)),
                            ),
                    ),
            )
    }

    fn context_fact(&self, label: &str, value: String) -> impl IntoElement {
        let palette = self.palette;
        div()
            .flex()
            .justify_between()
            .gap_2()
            .py(px(5.0))
            .border_t_1()
            .border_color(palette.line)
            .child(self.text(label.to_owned(), 12.0, palette.muted).flex_none())
            .child(self.text(value, 12.0, palette.text).min_w_0().text_right())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_usage_omits_a_ratio_for_incomplete_input_data() {
        let metrics = loaded_usage(&[
            serde_json::json!({"role":"assistant", "usage":{"input":8,"output":1,"cacheRead":4,"cacheWrite":0}}),
            serde_json::json!({"role":"assistant", "usage":{"output":8,"cacheRead":6}}),
        ]).unwrap();
        assert_eq!(metrics.cached, Some(10.0));
        assert_eq!(metrics.output, Some(9.0));
        assert_eq!(metrics.cache_percent, None);
    }

    #[test]
    fn loaded_usage_sums_only_assistant_messages() {
        let history = [
            serde_json::json!({"role":"assistant", "usage":{"input":10,"output":2,"cacheRead":20,"cacheWrite":4}}),
            serde_json::json!({"message":{"role":"assistant", "usage":{"input":6,"output":3,"cacheRead":40,"cacheWrite":0}}}),
            serde_json::json!({"role":"user", "usage":{"output":500}}),
        ];
        let metrics = loaded_usage(&history).unwrap();
        assert_eq!(metrics.input, Some(80.0));
        assert_eq!(metrics.output, Some(5.0));
        assert_eq!(metrics.cached, Some(60.0));
        assert_eq!(metrics.writes, Some(4.0));
        assert_eq!(metrics.cache_percent, Some(75.0));
    }

    #[test]
    fn elapsed_requires_start_and_settlement_on_the_same_connection() {
        let start = Instant::now();
        let end = start + Duration::from_millis(2400);
        let mut timing = RunTiming::default();
        timing.observe("agent_settled", end);
        assert_eq!(timing.elapsed(), None);

        timing.observe("agent_start", start);
        timing.observe("agent_start", start + Duration::from_secs(1));
        timing.observe("agent_end", end);
        assert_eq!(timing.elapsed(), None);
        timing.observe("agent_settled", end);
        assert_eq!(timing.elapsed().as_deref(), Some("2.4 s"));
        timing.observe("operation_accepted", end);
        assert_eq!(timing.elapsed(), None);

        timing.observe("agent_start", start);
        timing.observe("connection_reconnecting", end);
        timing.observe("connection_online", end);
        timing.observe("agent_settled", end);
        assert_eq!(timing.elapsed(), None);
    }
}
