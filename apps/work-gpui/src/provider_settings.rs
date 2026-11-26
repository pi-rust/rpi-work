//! Node-owned provider settings and write-only credentials.

use gpui::{Context, Div, FontWeight, Task, Window, div, prelude::*, px};
use work_core::api::{
    CredentialLocation, ProviderAction, ProviderCatalog, ProviderEntry, ProviderMutation,
    StoredCredential,
};

use super::Shell;

#[derive(Default)]
pub(super) struct Panel {
    pending: Option<Task<()>>,
    catalog: Option<Result<ProviderCatalog, ()>>,
    editor: Option<(String, CredentialChange)>,
    notice: Option<Notice>,
    needs_refresh: bool,
    writing: bool,
}

#[derive(Clone, Copy)]
enum CredentialChange {
    SetKey,
    Clear(CredentialLocation),
}

impl CredentialChange {
    fn available(self, entry: &ProviderEntry) -> bool {
        match self {
            Self::SetKey => true,
            Self::Clear(CredentialLocation::Stored) => {
                entry.stored_credential != StoredCredential::None
            }
            Self::Clear(CredentialLocation::Config) => entry.config_key_present,
        }
    }

    fn observed(self, entry: Option<&ProviderEntry>) -> bool {
        match self {
            Self::SetKey => {
                entry.is_some_and(|entry| entry.stored_credential == StoredCredential::ApiKey)
            }
            Self::Clear(location) => {
                entry.is_none_or(|entry| !Self::Clear(location).available(entry))
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Notice {
    Saved { requires_restart: bool },
    Unconfirmed,
    InvalidKey,
}

impl Panel {
    pub(super) fn editing_credential(&self) -> bool {
        self.editor.is_some()
    }

    pub(super) fn masking_credential(&self) -> bool {
        matches!(self.editor, Some((_, CredentialChange::SetKey)))
    }

    pub(super) fn leave(&mut self) {
        self.editor = None;
        if matches!(self.notice, Some(Notice::InvalidKey)) {
            self.notice = None;
        }
    }

    fn writable_catalog(&self) -> Option<&ProviderCatalog> {
        if self.pending.is_some() || self.needs_refresh {
            return None;
        }
        self.catalog
            .as_ref()?
            .as_ref()
            .ok()
            .filter(|catalog| !catalog.revision.is_empty())
    }
}

impl Shell {
    pub(super) fn read_provider_catalog(&mut self, cx: &mut Context<Self>) {
        if self.provider_settings.pending.is_some() || self.provider_settings.editing_credential() {
            return;
        }
        let Some(node) = &self.node else {
            return;
        };
        let read = node.read_provider_catalog();
        self.provider_settings.catalog = None;
        self.provider_settings.pending = Some(cx.spawn(async move |this, cx| {
            let result = read.await.ok().and_then(Result::ok).ok_or(());
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.provider_settings;
                panel.pending = None;
                if result.is_ok() {
                    panel.needs_refresh = false;
                }
                panel.catalog = Some(result);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open_credential_editor(
        &mut self,
        id: &str,
        change: CredentialChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = &mut self.provider_settings;
        if panel.editor.is_some()
            || !panel.writable_catalog().is_some_and(|catalog| {
                catalog
                    .providers
                    .iter()
                    .any(|entry| entry.id == id && change.available(entry))
            })
        {
            return;
        }
        panel.editor = Some((id.to_owned(), change));
        panel.notice = None;
        self.search.clear();
        self.show_settings_form();
        if self.provider_settings.masking_credential() {
            self.search.focus.focus(window);
        } else {
            self.focus_settings_title(window);
        }
        cx.notify();
    }

    pub(super) fn cancel_provider_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.provider_settings.leave();
        self.search.clear();
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(super) fn submit_provider_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.provider_settings.masking_credential() {
            self.submit_credential_change(window, cx);
        }
    }

    fn submit_credential_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = &mut self.provider_settings;
        let Some((id, change)) = panel.editor.clone() else {
            return;
        };
        let Some(catalog) = panel.writable_catalog() else {
            return;
        };
        let Some(node) = &self.node else { return };
        if !self.search.buffer.is_committed() {
            return;
        }
        let action = match change {
            CredentialChange::SetKey => {
                let key = std::mem::take(&mut self.search.buffer.text);
                self.search.clear();
                if key.trim().is_empty() || key.len() > 8192 || key.chars().any(char::is_control) {
                    panel.notice = Some(Notice::InvalidKey);
                    cx.notify();
                    return;
                }
                ProviderAction::SetKey {
                    id: id.clone(),
                    key,
                }
            }
            CredentialChange::Clear(location) => ProviderAction::ClearCredential {
                id: id.clone(),
                location,
            },
        };
        let write = node.mutate_provider(ProviderMutation {
            expected_revision: catalog.revision.clone(),
            action,
        });
        panel.editor = None;
        panel.notice = None;
        panel.writing = true;
        panel.pending = Some(cx.spawn(async move |this, cx| {
            let result = write.await.ok();
            let _ = this.update(cx, |shell, cx| {
                let panel = &mut shell.provider_settings;
                panel.pending = None;
                panel.writing = false;
                let (receipt, observed) =
                    result.map_or((None, None), |result| (result.receipt, result.observed));
                let confirmed = receipt.filter(|receipt| {
                    observed.as_ref().is_some_and(|catalog| {
                        !catalog.revision.is_empty()
                            && catalog.revision == receipt.catalog.revision
                            && change
                                .observed(catalog.providers.iter().find(|entry| entry.id == id))
                    })
                });
                panel.needs_refresh = confirmed.is_none();
                panel.notice = Some(match confirmed {
                    Some(receipt) => Notice::Saved {
                        requires_restart: receipt.requires_restart,
                    },
                    None => Notice::Unconfirmed,
                });
                panel.catalog = Some(observed.ok_or(()));
                cx.notify();
            });
        }));
        self.search.clear();
        self.focus_settings_title(window);
        cx.notify();
    }

    pub(super) fn provider_settings(&self, cx: &mut Context<Self>) -> Div {
        let panel = &self.provider_settings;
        if let Some((id, change)) = &panel.editor {
            return self.provider_credential_form(id, *change, cx);
        }
        let palette = self.palette;
        let pending = panel.pending.is_some();
        let body = div().flex().flex_col().gap_3().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    self.text("节点模型供应商", 15.0, palette.text)
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .when(self.node.is_some(), |heading| {
                    heading.child(
                        self.button("refresh-providers", "刷新")
                            .h(px(44.0))
                            .border_1()
                            .border_color(palette.line)
                            .tab_index(0)
                            .focus(|button| button.bg(palette.hover))
                            .when(pending, |button| button.opacity(0.6))
                            .on_click(
                                cx.listener(|shell, _, _, cx| shell.read_provider_catalog(cx)),
                            ),
                    )
                }),
        );
        let body = body.when_some(panel.notice, |body, notice| {
            let message = match notice {
                Notice::Saved {
                    requires_restart: true,
                } => "已回读确认保存状态。新宿主会读取更新后的配置。",
                Notice::Saved {
                    requires_restart: false,
                } => "已回读确认保存状态。",
                Notice::Unconfirmed if panel.needs_refresh => {
                    "上次操作结果待核对。请刷新目录，检查已保存的凭据来源。"
                }
                Notice::Unconfirmed => "目录已刷新。上次操作仍缺少可核对的回执。",
                Notice::InvalidKey => return body,
            };
            body.child(self.text(message, 13.0, palette.muted))
        });
        let body = body.when(panel.writing, |body| {
            body.child(self.text("正在保存并核对凭据状态…", 13.0, palette.muted))
        });
        match &panel.catalog {
            None if self.node.is_none() => {
                body.child(self.text("连接节点后可查看供应商。", 13.0, palette.muted))
            }
            None if pending => body.child(self.text("正在读取节点配置…", 13.0, palette.muted)),
            None => body,
            Some(Err(())) => body.child(self.text(
                "无法读取节点的供应商配置。可刷新重试。",
                13.0,
                palette.danger,
            )),
            Some(Ok(catalog)) if catalog.providers.is_empty() => {
                body.child(self.text("节点未报告已保存的供应商配置。", 13.0, palette.muted))
            }
            Some(Ok(catalog)) => body.children(
                catalog
                    .providers
                    .iter()
                    .map(|provider| self.provider_row(provider, cx)),
            ),
        }
    }

    fn provider_row(&self, provider: &ProviderEntry, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let name = provider
            .definition
            .as_ref()
            .map(|definition| definition.name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(&provider.id);
        let credential = match (provider.stored_credential, provider.config_key_present) {
            (StoredCredential::None, false) => "未报告已保存的凭据",
            (StoredCredential::None, true) => "定义含凭据配置",
            (StoredCredential::ApiKey, _) => "节点已保存 API 密钥",
            (StoredCredential::OAuth, _) => "节点已保存 OAuth 凭据",
            (StoredCredential::Other, _) => "节点已保存凭据",
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .border_t_1()
            .border_color(palette.line)
            .py_2()
            .child(self.text(name.to_owned(), 14.0, palette.text))
            .child(self.text(credential, 12.0, palette.muted))
            .when_some(provider.definition.as_ref(), |row, definition| {
                row.children(definition.models.iter().map(|model| {
                    let name = model.name.trim();
                    self.text(
                        if name.is_empty() { &model.id } else { name }.to_owned(),
                        13.0,
                        palette.text,
                    )
                }))
            })
            .when(
                provider.configured && provider.definition.is_none(),
                |row| row.child(self.text("节点未提供可显示的模型定义。", 12.0, palette.muted)),
            )
            .when(self.provider_settings.writable_catalog().is_some(), |row| {
                let set_label = if provider.stored_credential == StoredCredential::None {
                    "添加 API 密钥"
                } else {
                    "替换已保存凭据"
                };
                row.child(
                    div().flex().flex_wrap().gap_2().children(
                        [
                            ("provider-key", set_label, CredentialChange::SetKey),
                            (
                                "provider-clear-stored",
                                "清除已保存凭据",
                                CredentialChange::Clear(CredentialLocation::Stored),
                            ),
                            (
                                "provider-clear-config",
                                "清除定义内凭据",
                                CredentialChange::Clear(CredentialLocation::Config),
                            ),
                        ]
                        .into_iter()
                        .filter(|(_, _, change)| change.available(provider))
                        .map(|(button, label, change)| {
                            let id = provider.id.clone();
                            self.provider_button(
                                gpui::SharedString::from(format!("{button}-{id}")),
                                label,
                            )
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.open_credential_editor(&id, change, window, cx)
                                },
                            ))
                        }),
                    ),
                )
            })
    }

    fn provider_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &'static str,
    ) -> gpui::Stateful<Div> {
        let palette = self.palette;
        self.button(id, label)
            .h(px(44.0))
            .border_1()
            .border_color(palette.line)
            .tab_index(0)
            .focus(|button| button.border_color(palette.strong_line))
    }

    fn provider_credential_form(
        &self,
        id: &str,
        change: CredentialChange,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let (heading, label) = match change {
            CredentialChange::SetKey => ("保存 API 密钥", "保存密钥"),
            CredentialChange::Clear(CredentialLocation::Stored) => ("清除已保存凭据", "确认清除"),
            CredentialChange::Clear(CredentialLocation::Config) => {
                ("清除供应商定义内的凭据", "确认清除")
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                self.text(heading, 15.0, palette.text)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(self.text(id.to_owned(), 13.0, palette.muted))
            .when(matches!(change, CredentialChange::SetKey), |form| {
                form.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(44.0))
                        .px_2()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(palette.strong_line)
                        .child(self.search_input_element(cx)),
                )
            })
            .when(
                matches!(self.provider_settings.notice, Some(Notice::InvalidKey)),
                |form| {
                    form.child(self.text(
                        "请输入非空 API 密钥，最多 8192 字节且不得含控制字符。",
                        13.0,
                        palette.danger,
                    ))
                },
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.provider_button("cancel-provider-edit", "取消")
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.cancel_provider_edit(window, cx)
                            })),
                    )
                    .child(
                        self.provider_button("submit-provider-credential", label)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.submit_credential_change(window, cx)
                            })),
                    ),
            )
    }
}
