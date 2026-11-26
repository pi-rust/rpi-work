//! Saved provider definitions reported by the current node.

use gpui::{Context, Div, FontWeight, Task, div, prelude::*, px};
use work_core::api::{ProviderCatalog, ProviderEntry, StoredCredential};

use super::Shell;

#[derive(Default)]
pub(super) struct Panel {
    pending: Option<Task<()>>,
    catalog: Option<Result<ProviderCatalog, ()>>,
}

impl Shell {
    pub(super) fn read_provider_catalog(&mut self, cx: &mut Context<Self>) {
        if self.provider_settings.pending.is_some() {
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
                shell.provider_settings.pending = None;
                shell.provider_settings.catalog = Some(result);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn provider_settings(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let pending = self.provider_settings.pending.is_some();
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
        match &self.provider_settings.catalog {
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
                    .map(|provider| self.provider_row(provider)),
            ),
        }
    }

    fn provider_row(&self, provider: &ProviderEntry) -> Div {
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
    }
}
