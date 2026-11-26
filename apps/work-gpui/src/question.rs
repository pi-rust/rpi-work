//! Native editing for node questions with multiple choices or a text answer.

use std::collections::HashSet;

use gpui::{Context, Div, Window, div, prelude::*, px};
use work_core::api::{PendingDialog, UiDialogPayload};

use super::{
    Shell,
    editor::Buffer,
    session_action::{Action, Answer},
};

pub(super) struct Question {
    dialog: PendingDialog,
    session: usize,
    selected: HashSet<String>,
    pub(super) input: Buffer,
}

struct Spec {
    title: String,
    options: Vec<(String, String)>,
    multiple: bool,
    text: bool,
}

impl Spec {
    fn from_dialog(dialog: &PendingDialog) -> Option<Self> {
        let ui = &dialog.ui;
        if !matches!(ui.purpose.as_deref(), None | Some("question")) {
            return None;
        }
        let nested = ui.raw.get("questions").map(|questions| {
            let items = questions.as_array()?;
            let [item] = items.as_slice() else {
                return None;
            };
            serde_json::from_value::<UiDialogPayload>(item.clone()).ok()
        });
        let inner = match nested.as_ref() {
            Some(value) => value.as_ref()?,
            None => ui,
        };
        if !matches!(inner.purpose.as_deref(), None | Some("question")) {
            return None;
        }
        let kind = inner
            .kind
            .as_deref()
            .or(ui.kind.as_deref())
            .unwrap_or("selector");
        if !matches!(kind, "selector" | "input") {
            return None;
        }
        let flag = |keys: &[&str], inner_value, outer_value| {
            if keys.iter().any(|key| inner.raw.get(key).is_some()) {
                inner_value
            } else {
                outer_value
            }
        };
        let multiple = kind == "selector"
            && flag(
                &["allowMultiple", "allow_multiple", "multiple"],
                inner.allow_multiple,
                ui.allow_multiple,
            );
        let text = kind == "input"
            || flag(
                &["allowFreeform", "allow_freeform", "allow_custom"],
                inner.allow_freeform,
                ui.allow_freeform,
            );
        if !multiple && !text && nested.is_none() {
            return None;
        }
        let option_source = if inner.raw.get("options").is_some() {
            inner
        } else {
            ui
        };
        if option_source
            .raw
            .get("options")
            .is_some_and(|value| !value.is_array())
        {
            return None;
        }
        let mut seen = HashSet::new();
        let options: Vec<_> = option_source
            .options
            .iter()
            .filter_map(|option| {
                let value = option.value.as_ref()?.as_str()?.to_owned();
                (!value.trim().is_empty() && seen.insert(value.clone()))
                    .then(|| (option.title.clone().unwrap_or_else(|| value.clone()), value))
            })
            .collect();
        let title = inner
            .question
            .as_ref()
            .or(inner.message.as_ref())
            .or(ui.question.as_ref())
            .or(ui.message.as_ref())?
            .clone();
        if (kind == "selector" && options.is_empty() && !text) || title.trim().is_empty() {
            return None;
        }
        Some(Self {
            title,
            options: if kind == "input" { Vec::new() } else { options },
            multiple,
            text,
        })
    }

    fn permits(&self, answer: &Answer) -> bool {
        match answer {
            Answer::Selection(values) => {
                !values.is_empty()
                    && (self.multiple || values.len() == 1)
                    && values.iter().collect::<HashSet<_>>().len() == values.len()
                    && values
                        .iter()
                        .all(|value| self.options.iter().any(|(_, option)| value == option))
            }
            Answer::Text(text) => self.text && !text.trim().is_empty(),
        }
    }

    fn answer(&self, question: &Question) -> Option<Answer> {
        let draft = &question.input;
        if !draft.is_committed() {
            return None;
        }
        let answer = if self.text && draft.can_send() {
            Answer::Text(draft.text.trim().to_owned())
        } else {
            Answer::Selection(
                self.options
                    .iter()
                    .filter_map(|(_, value)| {
                        question.selected.contains(value).then_some(value.clone())
                    })
                    .collect(),
            )
        };
        self.permits(&answer).then_some(answer)
    }
}

impl Question {
    pub(super) fn supported(dialog: &PendingDialog) -> bool {
        Spec::from_dialog(dialog).is_some()
    }

    pub(super) fn permits(dialog: &PendingDialog, answer: &Answer) -> bool {
        Spec::from_dialog(dialog).is_some_and(|spec| spec.permits(answer))
    }

    pub(super) fn current(&self, session: usize, dialogs: &[PendingDialog]) -> bool {
        self.session == session && dialogs.contains(&self.dialog)
    }
}

impl Shell {
    pub(super) fn open_question(
        &mut self,
        dialog: PendingDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session else { return };
        if !(self.session_controls().reply
            && self.action_available()
            && Question::supported(&dialog)
            && self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.dialogs.contains(&dialog)))
        {
            return;
        }
        if let Some(draft) = self.drafts.get_mut(&self.draft_key()) {
            draft.input.finish_composition();
        }
        self.question = Some(Question {
            dialog,
            session,
            selected: HashSet::new(),
            input: Buffer::default(),
        });
        self.input_layout = None;
        self.input_dragging = false;
        self.focus_composer(window);
        cx.notify();
    }

    pub(super) fn close_question(&mut self) -> bool {
        if self.question.take().is_none() {
            return false;
        }
        self.input_layout = None;
        self.input_dragging = false;
        true
    }

    fn submit_question(&mut self, cx: &mut Context<Self>) {
        let Some(question) = self.question.as_ref() else {
            return;
        };
        if !self
            .frame
            .as_ref()
            .zip(self.session)
            .is_some_and(|(frame, session)| question.current(session, &frame.dialogs))
        {
            return;
        }
        let Some(answer) =
            Spec::from_dialog(&question.dialog).and_then(|spec| spec.answer(question))
        else {
            return;
        };
        let session = Some(question.session);
        let action = Action::Reply {
            request_id: question.dialog.request_id.clone(),
            answer,
        };
        self.start_control(session, action, cx);
    }

    pub(super) fn question_dialog(
        &self,
        question: &Question,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let spec = Spec::from_dialog(&question.dialog)?;
        let palette = self.palette;
        let editable = self.session_controls().reply && self.action_available();
        let notice = self
            .actions
            .get(&question.session)
            .map(|request| request.status().to_owned());
        Some(
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
                        .id("question-dialog")
                        .when(!spec.text || !editable, |dialog| {
                            dialog.track_focus(&self.focus)
                        })
                        .flex()
                        .flex_col()
                        .gap_3()
                        .w_full()
                        .max_w(px(480.0))
                        .max_h_full()
                        .overflow_y_scroll()
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
                                .child(self.text("回复节点问题", 16.0, palette.text))
                                .child(self.icon_button("close-question", "x", 24.0).on_click(
                                    cx.listener(|shell, _, _, cx| {
                                        shell.close_question();
                                        cx.notify();
                                    }),
                                )),
                        )
                        .child(self.text(spec.title.clone(), 14.0, palette.text))
                        .when_some(question.dialog.ui.context.as_ref(), |panel, context| {
                            panel.child(self.text(context.clone(), 12.0, palette.muted))
                        })
                        .children(spec.options.iter().enumerate().map(
                            |(index, (title, choice))| {
                                let selected = question.selected.contains(choice);
                                let value = choice.clone();
                                let multiple = spec.multiple;
                                self.button(
                                    "question-choice",
                                    &format!("{} {title}", if selected { "✓" } else { "○" }),
                                )
                                .id(("question-choice", index))
                                .justify_start()
                                .when(selected, |button| {
                                    button
                                        .bg(palette.hover)
                                        .border_1()
                                        .border_color(palette.strong_line)
                                })
                                .when(editable, |button| {
                                    button.on_click(cx.listener(move |shell, _, _, cx| {
                                        if let Some(question) = shell.question.as_mut() {
                                            if !question.selected.remove(&value) {
                                                if !multiple {
                                                    question.selected.clear();
                                                }
                                                question.selected.insert(value.clone());
                                            }
                                            cx.notify();
                                        }
                                    }))
                                })
                            },
                        ))
                        .when(spec.text, |dialog| {
                            dialog.child(
                                div()
                                    .border_1()
                                    .border_color(palette.strong_line)
                                    .rounded(px(7.0))
                                    .p_2()
                                    .when(editable, |input| input.child(self.input_element(cx)))
                                    .when(!editable, |input| {
                                        input.child(self.text(
                                            question.input.text.clone(),
                                            14.0,
                                            palette.muted,
                                        ))
                                    }),
                            )
                        })
                        .when_some(notice, |dialog, notice| {
                            dialog.child(self.text(notice, 12.0, palette.muted))
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .when(
                                    self.actions
                                        .get(&question.session)
                                        .is_some_and(|request| request.can_check()),
                                    |bar| {
                                        bar.child(
                                            self.button("check-question", "核对操作回执").on_click(
                                                cx.listener(|shell, _, _, cx| {
                                                    shell.check_control(cx)
                                                }),
                                            ),
                                        )
                                    },
                                )
                                .child(self.button("submit-question", "提交回答").when(
                                    editable && spec.answer(question).is_some(),
                                    |button| {
                                        button.bg(palette.subtle).on_click(
                                            cx.listener(|shell, _, _, cx| {
                                                shell.submit_question(cx)
                                            }),
                                        )
                                    },
                                )),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn question_answers_validate_exact_values_and_text_permissions() {
        let mut dialog: PendingDialog = serde_json::from_value(json!({
            "requestId":"q", "ui":{"kind":"selector", "purpose":"question", "questions":[{
                "question":"Choose", "allowMultiple":true,
                "options":[{"title":"Label A","value":"a "},{"title":"Label B","value":"b"}]
            }]}
        }))
        .unwrap();
        let allowed = Answer::Selection(vec!["a ".into(), "b".into()]);
        assert!(Question::permits(&dialog, &allowed));
        assert_eq!(
            allowed.payload(),
            json!({"kind":"selector","values":["a ","b"]})
        );
        assert!(!Question::permits(
            &dialog,
            &Answer::Selection(vec!["Label A".into()])
        ));
        assert!(!Question::permits(
            &dialog,
            &Answer::Selection(vec!["b".into(), "b".into()])
        ));
        assert!(!Question::permits(&dialog, &Answer::Text("else".into())));
        dialog.ui.purpose = Some("permission".into());
        assert!(!Question::supported(&dialog));
        let dialog: PendingDialog = serde_json::from_value(json!({
            "requestId":"q", "ui":{"kind":"input", "question":"Write"}
        }))
        .unwrap();
        let text = Answer::Text("回答 🌊".into());
        assert!(Question::permits(&dialog, &text));
        assert_eq!(
            text.payload(),
            json!({"kind":"input","values":["回答 🌊"],"value":"回答 🌊","text":"回答 🌊"})
        );
    }
}
