//! Read-only facts about the selected session.

use gpui::{Context, FocusHandle, FontWeight, Task, Window, div, prelude::*, px};
use work_client::RuntimeState;
use work_viewmodel::observability::metrics::{Context as Usage, grouped, percent};

use super::{Shell, composer_config::thinking_label};

pub(super) struct Panel {
    pub(super) open: bool,
    close: FocusHandle,
    read: Option<Task<()>>,
    runtime: Option<RuntimeState>,
}

impl Panel {
    pub(super) fn new(cx: &Context<Shell>) -> Self {
        Self {
            open: false,
            close: cx.focus_handle(),
            read: None,
            runtime: None,
        }
    }

    pub(super) fn reset(&mut self) {
        self.open = false;
        self.read = None;
        self.runtime = None;
    }
}

impl Shell {
    pub(super) fn open_session_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_none()
            || !self.navigation_keyboard_enabled()
            || self.composer_is_marked()
        {
            return;
        }
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
        let (history, usage) = if self.node.is_none() && self.session == self.transcript_session {
            (
                Some(self.snapshot.history.as_slice()),
                self.snapshot.context.clone(),
            )
        } else {
            (
                self.frame.as_ref().map(|frame| frame.history.as_slice()),
                Usage::reported(runtime.and_then(|state| state.context.as_ref())),
            )
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
        let location = if self.node.as_ref().is_some_and(|node| node.is_local()) {
            "本机节点"
        } else {
            self.snapshot.node.as_str()
        };
        let mut fields = vec![
            (
                "项目",
                self.project
                    .map_or(missing, |index| self.snapshot.projects[index].name.as_str())
                    .to_owned(),
            ),
            ("执行位置", location.to_owned()),
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
        if usage.tokens.is_some() || usage.window.is_some() {
            fields.push((
                "上下文窗口",
                format!("{} / {}", grouped(usage.tokens), grouped(usage.window)),
            ));
        }
        if usage.percent.is_some() {
            fields.push(("上下文使用率", percent(usage.percent)));
        }
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
                            .children(fields.into_iter().map(|(label, value)| {
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap_3()
                                    .py(px(10.0))
                                    .border_t_1()
                                    .border_color(palette.line)
                                    .child(self.text(label, 13.0, palette.muted).flex_none())
                                    .child(
                                        self.text(value, 13.0, palette.text).min_w_0().text_right(),
                                    )
                            })),
                    ),
            )
    }
}
