//! Edit the node's projected provider definitions using the workbench text input.

use gpui::{Context, Div, Window, div, prelude::*, px};
use work_core::api::{
    ProviderAction, ProviderApi, ProviderDefinition, ProviderEntry, ProviderModel,
};

use super::{Panel, Shell};

pub(super) enum Editor {
    Definition(Box<Draft>),
    Remove(String),
}

pub(super) struct Draft {
    original: Option<ProviderDefinition>,
    value: ProviderDefinition,
    model: Option<usize>,
    saved_models: usize,
    field: Option<Field>,
    error: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Id,
    Name,
    Endpoint,
    ModelId,
    ModelName,
    ContextWindow,
    MaxTokens,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Id => "供应商 ID",
            Self::Name => "供应商名称",
            Self::Endpoint => "服务地址",
            Self::ModelId => "模型 ID",
            Self::ModelName => "模型名称",
            Self::ContextWindow => "上下文窗口",
            Self::MaxTokens => "最大输出 Token",
        }
    }
}

impl Draft {
    fn new(original: Option<ProviderDefinition>) -> Self {
        let value = original.clone().unwrap_or_else(|| ProviderDefinition {
            id: String::new(),
            name: String::new(),
            api: ProviderApi::OpenaiCompletions,
            base_url: String::new(),
            models: Vec::new(),
        });
        Self {
            saved_models: value.models.len(),
            original,
            value,
            model: None,
            field: None,
            error: None,
        }
    }

    fn model(&self) -> &ProviderModel {
        &self.value.models[self.model.expect("model editor is open")]
    }

    fn model_mut(&mut self) -> &mut ProviderModel {
        &mut self.value.models[self.model.expect("model editor is open")]
    }

    fn value(&self, field: Field) -> String {
        match field {
            Field::Id => self.value.id.clone(),
            Field::Name => self.value.name.clone(),
            Field::Endpoint => self.value.base_url.clone(),
            Field::ModelId => self.model().id.clone(),
            Field::ModelName => self.model().name.clone(),
            Field::ContextWindow => self
                .model()
                .context_window
                .map(|v| v.to_string())
                .unwrap_or_default(),
            Field::MaxTokens => self
                .model()
                .max_tokens
                .map(|v| v.to_string())
                .unwrap_or_default(),
        }
    }

    fn set(&mut self, field: Field, value: String) -> Result<(), &'static str> {
        match field {
            Field::Id => self.value.id = value,
            Field::Name => self.value.name = value,
            Field::Endpoint => self.value.base_url = value,
            Field::ModelId => self.model_mut().id = value,
            Field::ModelName => self.model_mut().name = value,
            Field::ContextWindow => self.model_mut().context_window = token_count(&value)?,
            Field::MaxTokens => self.model_mut().max_tokens = token_count(&value)?,
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), &'static str> {
        let definition = &self.value;
        if definition.id.is_empty()
            || definition.id.len() > 96
            || matches!(definition.id.as_str(), "." | "..")
            || !definition
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err("供应商 ID 须为 1–96 个字母、数字或 -_.，且不能为 . 或 ..。");
        }
        if !valid_text(&definition.name, 160) {
            return Err("供应商名称最多 160 字节，且不能包含控制字符。");
        }
        let url = &definition.base_url;
        let authority = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        if authority.is_empty()
            || authority.contains('@')
            || url.contains(['?', '#'])
            || url.len() > 2048
            || url.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        {
            return Err("请输入 HTTP(S) 服务地址，地址中不能包含凭据、查询、片段或空白。");
        }
        let mut ids = std::collections::BTreeSet::new();
        if definition.models.iter().any(|model| {
            model.id.is_empty()
                || model.id.trim() != model.id
                || !valid_text(&model.id, 200)
                || !valid_text(&model.name, 200)
                || !ids.insert(&model.id)
        }) {
            return Err("模型 ID 须唯一且无首尾空白，ID 和名称最多 200 字节。");
        }
        if definition.models.iter().any(|model| {
            model
                .context_window
                .zip(model.max_tokens)
                .is_some_and(|(window, max)| max > window)
        }) {
            return Err("最大输出 Token 不能超过上下文窗口。");
        }
        Ok(())
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(char::is_control)
}

fn token_count(value: &str) -> Result<Option<u64>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match value.parse::<u64>() {
        Ok(count @ 1..=100_000_000) => Ok(Some(count)),
        _ => Err("Token 数须为 1–100000000 的整数，可留空。"),
    }
}

impl Panel {
    pub(in crate::shell) fn field_label(&self) -> Option<&'static str> {
        match self.definition.as_ref()? {
            Editor::Definition(draft) => draft.field.map(Field::label),
            Editor::Remove(_) => None,
        }
    }

    pub(super) fn can_resume_definition(&self) -> bool {
        let Some(catalog) = self.writable_catalog() else {
            return false;
        };
        let Some(draft) = self.unconfirmed_definition.as_ref() else {
            return false;
        };
        let current = catalog
            .providers
            .iter()
            .find(|entry| entry.id == draft.value.id);
        match draft.original.as_ref() {
            Some(original) => current.and_then(|entry| entry.definition.as_ref()) == Some(original),
            None => current.is_none_or(|entry| !entry.configured),
        }
    }

    fn definition_draft(&mut self) -> Option<&mut Draft> {
        match self.definition.as_mut()? {
            Editor::Definition(draft) => Some(draft.as_mut()),
            Editor::Remove(_) => None,
        }
    }
}

impl Shell {
    pub(super) fn resume_provider_definition(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = &mut self.provider_settings;
        if panel.editing() || !panel.can_resume_definition() {
            return;
        }
        panel.definition = panel.unconfirmed_definition.take().map(Editor::Definition);
        self.search.clear();
        self.show_settings_form();
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(super) fn open_provider_definition(
        &mut self,
        id: Option<&str>,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = &mut self.provider_settings;
        let Some(catalog) = panel.writable_catalog() else {
            return;
        };
        if panel.editing() {
            return;
        }
        panel.definition = Some(match id {
            None => Editor::Definition(Box::new(Draft::new(None))),
            Some(id) => {
                let Some(entry) = catalog.providers.iter().find(|entry| entry.id == id) else {
                    return;
                };
                if remove && entry.configured {
                    Editor::Remove(id.to_owned())
                } else if let Some(definition) = &entry.definition {
                    Editor::Definition(Box::new(Draft::new(Some(definition.clone()))))
                } else {
                    return;
                }
            }
        });
        panel.notice = None;
        self.search.clear();
        self.show_settings_form();
        self.focus_settings_title(window);
        cx.notify();
    }

    fn commit_provider_field(&mut self) -> bool {
        let Some(draft) = self.provider_settings.definition_draft() else {
            return false;
        };
        let Some(field) = draft.field else {
            return true;
        };
        if !self.search.buffer.is_committed() {
            return false;
        }
        draft.error = draft.set(field, self.search.buffer.text.clone()).err();
        if draft.error.is_some() {
            return false;
        }
        draft.field = None;
        self.search.clear();
        true
    }

    pub(in crate::shell) fn finish_provider_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commit_provider_field() {
            self.focus_settings_title(window);
        }
        cx.notify();
    }

    fn edit_provider_field(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_provider_field()
            && let Some(draft) = self.provider_settings.definition_draft()
        {
            let value = draft.value(field);
            draft.field = Some(field);
            draft.error = None;
            self.search.buffer.replace(None, &value);
            self.search.buffer.select_all();
            self.search.focus.focus(window);
        }
        cx.notify();
    }

    fn show_provider_model(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_provider_field()
            && let Some(draft) = self.provider_settings.definition_draft()
        {
            if index == draft.value.models.len() && index < 128 {
                draft.value.models.push(ProviderModel {
                    id: String::new(),
                    name: String::new(),
                    reasoning: false,
                    context_window: None,
                    max_tokens: None,
                });
            }
            if index < draft.value.models.len() {
                draft.model = Some(index);
                draft.error = None;
                self.show_settings_form();
                self.focus_settings_title(window);
            }
        }
        cx.notify();
    }

    fn close_provider_model(&mut self, remove: bool, window: &mut Window, cx: &mut Context<Self>) {
        if (remove || self.commit_provider_field())
            && let Some(draft) = self.provider_settings.definition_draft()
            && let Some(index) = draft.model.take()
        {
            if remove {
                draft.value.models.remove(index);
                if index < draft.saved_models {
                    draft.saved_models -= 1;
                }
            }
            draft.field = None;
            draft.error = None;
            self.search.clear();
            self.show_settings_form();
            self.focus_settings_title(window);
        }
        cx.notify();
    }

    fn save_provider_definition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Editor::Remove(id)) = &self.provider_settings.definition {
            let id = id.clone();
            self.write_provider(
                ProviderAction::RemoveConfig { id: id.clone() },
                id,
                |entry| entry.is_none_or(|entry| !entry.configured),
                window,
                cx,
            );
            return;
        }
        if !self.commit_provider_field() {
            cx.notify();
            return;
        }
        let panel = &mut self.provider_settings;
        let Some(catalog) = panel.writable_catalog() else {
            return;
        };
        let Some(Editor::Definition(draft)) = &panel.definition else {
            return;
        };
        let error = draft.validate().err().or_else(|| {
            (draft.original.is_none()
                && catalog
                    .providers
                    .iter()
                    .any(|entry| entry.id == draft.value.id && entry.configured))
            .then_some("此 ID 已有供应商定义，请从目录打开对应条目。")
        });
        if let Some(error) = error {
            if let Some(draft) = panel.definition_draft() {
                draft.error = Some(error);
            }
            cx.notify();
            return;
        }
        let definition = draft.value.clone();
        self.write_provider(
            ProviderAction::Upsert {
                definition: definition.clone(),
            },
            definition.id.clone(),
            move |entry| entry.and_then(|entry| entry.definition.as_ref()) == Some(&definition),
            window,
            cx,
        );
    }

    pub(super) fn provider_definition_actions(
        &self,
        entry: &ProviderEntry,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        [
            (false, "编辑定义", entry.definition.is_some()),
            (true, "移除定义", entry.configured),
        ]
        .into_iter()
        .filter(|(_, _, visible)| *visible)
        .map(|(remove, label, _)| {
            let id = entry.id.clone();
            self.provider_button(
                gpui::SharedString::from(format!("provider-definition-{remove}-{id}")),
                label,
            )
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_provider_definition(Some(&id), remove, window, cx)
            }))
        })
        .collect()
    }

    pub(super) fn provider_definition_form(&self, editor: &Editor, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let body = div().flex().flex_col().gap_3();
        let draft = match editor {
            Editor::Definition(draft) => draft,
            Editor::Remove(id) => {
                return body
                    .child(self.text("移除供应商定义", 16.0, palette.text))
                    .child(self.text(id.clone(), 13.0, palette.muted))
                    .child(self.text(
                        "此操作会移除定义、其中的模型及定义内的凭据。单独保存的凭据会保留。",
                        13.0,
                        palette.muted,
                    ))
                    .child(self.provider_definition_footer("确认移除", cx));
            }
        };
        let body = body.when_some(draft.error, |form, error| {
            form.child(self.text(error, 13.0, palette.danger))
        });
        if draft.model.is_some() {
            self.provider_model_form(body, draft, cx)
        } else {
            let label = if draft.original.is_some() {
                "编辑供应商"
            } else {
                "添加供应商"
            };
            body.child(self.text(label, 16.0, palette.text))
                .child(self.provider_field(draft, Field::Id, draft.original.is_none(), cx))
                .child(self.provider_field(draft, Field::Name, true, cx))
                .child(self.provider_field(draft, Field::Endpoint, true, cx))
                .child(self.text("API 协议", 13.0, palette.text))
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            (
                                "provider-api-completions",
                                "OpenAI Completions",
                                ProviderApi::OpenaiCompletions,
                            ),
                            (
                                "provider-api-responses",
                                "OpenAI Responses",
                                ProviderApi::OpenaiResponses,
                            ),
                            (
                                "provider-api-anthropic",
                                "Anthropic Messages",
                                ProviderApi::AnthropicMessages,
                            ),
                        ]
                        .map(|(id, label, api)| {
                            self.provider_button(id, label)
                                .when(draft.value.api == api, |button| {
                                    button.bg(palette.subtle).border_color(palette.strong_line)
                                })
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    if let Some(draft) = shell.provider_settings.definition_draft()
                                    {
                                        draft.value.api = api;
                                        cx.notify();
                                    }
                                }))
                        }),
                    ),
                )
                .child(self.provider_definition_footer("保存定义", cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(self.text("模型", 14.0, palette.text))
                        .when(draft.value.models.len() < 128, |row| {
                            let index = draft.value.models.len();
                            row.child(
                                self.provider_button("provider-add-model", "添加模型")
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.show_provider_model(index, window, cx)
                                    })),
                            )
                        }),
                )
                .children(draft.value.models.iter().enumerate().map(|(index, model)| {
                    let label = if model.id.is_empty() {
                        "未填写模型 ID"
                    } else {
                        &model.id
                    };
                    self.button(("provider-model", index), label)
                        .h(px(44.0))
                        .justify_start()
                        .border_1()
                        .border_color(palette.line)
                        .tab_index(0)
                        .focus(|button| button.border_color(palette.strong_line))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_provider_model(index, window, cx)
                        }))
                }))
        }
    }

    fn provider_model_form(&self, body: Div, draft: &Draft, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let model = draft.model();
        let id_editable = draft.model.is_some_and(|index| index >= draft.saved_models);
        body.child(self.text("模型定义", 16.0, palette.text))
            .child(self.provider_field(draft, Field::ModelId, id_editable, cx))
            .child(self.provider_field(draft, Field::ModelName, true, cx))
            .child(self.provider_field(draft, Field::ContextWindow, true, cx))
            .child(self.provider_field(draft, Field::MaxTokens, true, cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        self.provider_button(
                            "provider-model-reasoning",
                            if model.reasoning {
                                "推理模型 · 开"
                            } else {
                                "推理模型 · 关"
                            },
                        )
                        .on_click(cx.listener(|shell, _, _, cx| {
                            if let Some(draft) = shell.provider_settings.definition_draft() {
                                let model = draft.model_mut();
                                model.reasoning = !model.reasoning;
                                cx.notify();
                            }
                        })),
                    )
                    .child(
                        self.provider_button("provider-remove-model", "从定义移除模型")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_provider_model(true, window, cx)
                            })),
                    )
                    .child(
                        self.provider_button("provider-back-to-definition", "返回供应商")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_provider_model(false, window, cx)
                            })),
                    ),
            )
    }

    fn provider_field(
        &self,
        draft: &Draft,
        field: Field,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let row = div().flex().items_center().gap_2().child(
            self.text(field.label(), 13.0, palette.text)
                .w(px(113.0))
                .flex_none(),
        );
        if draft.field == Some(field) {
            row.child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .h(px(44.0))
                    .px_2()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(palette.strong_line)
                    .child(self.search_input_element(cx)),
            )
            .child(
                self.provider_button("provider-field-apply", "完成")
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.finish_provider_field(window, cx)),
                    ),
            )
        } else {
            let mut value = draft.value(field);
            if value.is_empty() {
                value = "未填写".into();
            }
            if editable {
                row.child(
                    self.button(field.label(), "")
                        .h(px(44.0))
                        .flex_1()
                        .min_w_0()
                        .justify_start()
                        .border_1()
                        .border_color(palette.line)
                        .tab_index(0)
                        .focus(|button| button.border_color(palette.strong_line))
                        .child(self.text(value, 13.0, palette.text).truncate())
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.edit_provider_field(field, window, cx)
                        })),
                )
            } else {
                row.child(self.text(value, 13.0, palette.muted).truncate())
            }
        }
    }

    fn provider_definition_footer(&self, label: &'static str, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                self.provider_button("cancel-provider-definition", "取消")
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.cancel_provider_edit(window, cx)),
                    ),
            )
            .child(
                self.provider_button("save-provider-definition", label)
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.save_provider_definition(window, cx)
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::api::{ProviderCatalog, StoredCredential};

    #[test]
    fn unconfirmed_edit_can_resume_only_against_its_original_definition() {
        let original = ProviderDefinition {
            id: "fixture".into(),
            name: "Original".into(),
            api: ProviderApi::OpenaiResponses,
            base_url: "https://fixture.invalid".into(),
            models: Vec::new(),
        };
        let mut draft = Draft::new(Some(original.clone()));
        draft.value.name = "Edited".into();
        let mut panel = Panel {
            unconfirmed_definition: Some(Box::new(draft)),
            needs_refresh: true,
            catalog: Some(Ok(ProviderCatalog {
                revision: "latest".into(),
                providers: vec![ProviderEntry {
                    id: original.id.clone(),
                    definition: Some(original),
                    configured: true,
                    config_key_present: false,
                    stored_credential: StoredCredential::None,
                    diagnostic: None,
                }],
            })),
            ..Panel::default()
        };
        assert!(!panel.can_resume_definition());
        panel.needs_refresh = false;
        assert!(panel.can_resume_definition());
        let catalog = panel.catalog.as_mut().unwrap().as_mut().unwrap();
        catalog.providers[0].definition.as_mut().unwrap().name = "Other writer".into();
        assert!(!panel.can_resume_definition());
    }
}
