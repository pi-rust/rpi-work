//! Line feedback anchored to a verified node patch.

use std::rc::Rc;

use gpui::{Context, Div, Window, div, prelude::*, px};
use work_client::Diff;
use work_viewmodel::diff_view;

use super::{
    Overlay, Shell,
    editor::Buffer,
    project_files::{Contents, Route},
};
use crate::prompt::Feedback;

pub(super) struct Comment {
    diff: Rc<Diff>,
    staged: bool,
    row: usize,
    location: String,
    pub(super) input: Buffer,
    verifying: bool,
    error: Option<String>,
}

impl Shell {
    pub(super) fn open_comment(
        &mut self,
        diff: Rc<Diff>,
        staged: bool,
        row: usize,
        location: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.comment.is_some() || self.frame.is_none() {
            return;
        }
        if let Some(draft) = self.drafts.get_mut(&self.session) {
            draft.input.finish_composition();
        }
        self.input_layout = None;
        self.input_dragging = false;
        self.comment = Some(Comment {
            diff,
            staged,
            row,
            location,
            input: Buffer::default(),
            verifying: false,
            error: None,
        });
        self.focus_composer(window);
        cx.notify();
    }

    pub(super) fn cancel_comment(&mut self) -> bool {
        if self.comment.take().is_none() {
            return false;
        }
        self.input_layout = None;
        self.input_dragging = false;
        true
    }

    fn verify_comment(&mut self, cx: &mut Context<Self>) {
        let Some((node, project)) = self.node.as_ref().zip(self.project) else {
            return;
        };
        let Some(comment) = self.comment.as_mut().filter(|comment| {
            !comment.verifying
                && comment.input.can_send()
                && comment.input.text.chars().count() <= 2000
        }) else {
            return;
        };
        comment.verifying = true;
        comment.error = None;
        let observed = Rc::clone(&comment.diff);
        let revision = comment.input.revision;
        let request = node.read_project(
            self.snapshot.projects[project].id.clone(),
            Route::Diff {
                path: observed.path.clone(),
                staged: comment.staged,
            },
        );
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(comment) = shell
                    .comment
                    .as_mut()
                    .filter(|comment| Rc::ptr_eq(&comment.diff, &observed))
                else {
                    return;
                };
                comment.verifying = false;
                let error = if comment.input.revision != revision || !comment.input.is_committed() {
                    "反馈已编辑，请重新确认。"
                } else if let Ok(Ok(Contents::Diff(fresh))) = result {
                    if let Some(item) = diff_view::feedback_draft(
                        &observed.patch,
                        &fresh.patch,
                        &observed.path,
                        comment.staged,
                        comment.row,
                        &comment.input.text,
                    ) {
                        if shell
                            .drafts
                            .entry(shell.session)
                            .or_default()
                            .add_feedback(item)
                        {
                            shell.cancel_comment();
                            shell.inspector_full = false;
                            if shell.overlay == Some(Overlay::Inspector) {
                                shell.overlay = None;
                            }
                            cx.notify();
                            return;
                        }
                        "草稿中的行内反馈已达数量或长度上限。"
                    } else {
                        "差异已变化。请关闭对话框，刷新差异后重新选择行。"
                    }
                } else {
                    "无法核对当前差异。请重试。"
                };
                comment.error = Some(error.into());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn comment_dialog(&self, comment: &Comment, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let over_limit = comment.input.text.chars().count() > 2000;
        let enabled = !comment.verifying && !over_limit && comment.input.can_send();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000039))
            .flex()
            .items_center()
            .justify_center()
            .px_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .max_w(px(440.0))
                    .p_4()
                    .bg(palette.bg)
                    .border_1()
                    .border_color(palette.line)
                    .rounded(px(12.0))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(self.text("行内反馈", 15.0, palette.text))
                            .child(self.icon_button("close-line-feedback", "x", 13.0).on_click(
                                cx.listener(|shell, _, _, cx| {
                                    shell.cancel_comment();
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(self.text(comment.location.clone(), 12.0, palette.muted))
                    .child(
                        div()
                            .border_1()
                            .border_color(palette.strong_line)
                            .rounded(px(7.0))
                            .p_2()
                            .child(self.input_element(cx)),
                    )
                    .when_some(
                        if over_limit {
                            Some("行内反馈最多 2000 字。")
                        } else {
                            comment.error.as_deref()
                        },
                        |dialog, message| {
                            dialog.child(self.text(message.to_owned(), 12.0, palette.danger))
                        },
                    )
                    .child(
                        div().flex().justify_end().child(
                            self.button(
                                "add-line-feedback",
                                if comment.verifying {
                                    "正在核对差异…"
                                } else {
                                    "加入下一条消息"
                                },
                            )
                            .when(enabled, |button| {
                                button
                                    .bg(palette.subtle)
                                    .border_1()
                                    .border_color(palette.strong_line)
                                    .on_click(
                                        cx.listener(|shell, _, _, cx| shell.verify_comment(cx)),
                                    )
                            })
                            .when(!enabled, |button| button.opacity(0.6)),
                        ),
                    ),
            )
    }

    pub(super) fn feedback_chips(
        &self,
        items: &[Feedback],
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        div()
            .id("feedback-draft-list")
            .when(!items.is_empty(), |list| {
                list.flex()
                    .flex_wrap()
                    .gap_1()
                    .max_h(px(100.0))
                    .overflow_y_scroll()
                    .pb_2()
            })
            .children(items.iter().map(|item| {
                let id = item.id;
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .max_w_full()
                    .pl_2()
                    .rounded(px(5.0))
                    .bg(palette.subtle)
                    .child(
                        self.text(item.location.clone(), 11.0, palette.text)
                            .truncate(),
                    )
                    .child(
                        self.icon_button("remove-line-feedback", "x", 11.0)
                            .id(("remove-line-feedback", id))
                            .flex_none()
                            .h(px(22.0))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                if let Some(draft) = shell.drafts.get_mut(&shell.session) {
                                    draft.feedback.retain(|item| item.id != id);
                                }
                                shell.focus_composer(window);
                                cx.notify();
                            })),
                    )
            }))
    }
}
