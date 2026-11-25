//! Observed model and thinking choices for the selected canonical session.

use std::collections::{HashMap, HashSet};

use gpui::{Context, Div, SharedString, Task, deferred, div, prelude::*, px};
use work_client::{ClientError, RuntimeState, SessionConfiguration, WorkspaceClient};
use work_core::api::realtime::OperationRef;
use work_viewmodel::config_requests::{CatalogAccess, ConfigField, bounded};
use work_viewmodel::session_controls::SessionControls;
use work_viewmodel::session_receipts::interpret_receipt;

use super::{Shell, rejected};
use crate::icons::icon;

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) enum Choice {
    Model { id: String, provider: String },
    Thinking(String),
}

impl Choice {
    fn field(&self) -> ConfigField {
        match self {
            Self::Model { .. } => ConfigField::Model,
            Self::Thinking(_) => ConfigField::Thinking,
        }
    }

    fn matches(&self, state: &RuntimeState) -> bool {
        match self {
            Self::Model { id, provider } => {
                state.model_id.as_ref() == Some(id) && state.provider.as_ref() == Some(provider)
            }
            Self::Thinking(level) => state.thinking_level.as_ref() == Some(level),
        }
    }
}

pub(crate) struct Observed {
    pub schema: SessionConfiguration,
    pub runtime: RuntimeState,
}

pub(crate) async fn observe(client: &WorkspaceClient, id: &str) -> Result<Observed, ClientError> {
    bounded("读取会话配置", async {
        let schema = client.session_configuration(id).await?;
        let catalog = CatalogAccess::for_configuration(&schema);
        if !catalog.models_read && !catalog.thinking_read {
            return Err(ClientError::Unsupported {
                operation: "composer catalog".into(),
                protocol: schema.protocol,
            });
        }
        let runtime = client.runtime_state(id).await?;
        Ok(Observed { schema, runtime })
    })
    .await
}

type Choices = Vec<(Choice, String)>;
pub(crate) type CatalogResult = Result<Choices, ClientError>;

pub(crate) async fn choices(
    client: &WorkspaceClient,
    id: &str,
    field: ConfigField,
) -> CatalogResult {
    let mut seen = HashSet::new();
    match field {
        ConfigField::Model => Ok(bounded("读取模型目录", client.models(id))
            .await?
            .into_iter()
            .filter(|item| !item.id.trim().is_empty() && !item.provider.trim().is_empty())
            .filter_map(|item| {
                let name = if item.name.trim().is_empty() {
                    &item.id
                } else {
                    &item.name
                };
                let label = format!("{name} · {}", item.provider);
                let choice = Choice::Model {
                    id: item.id,
                    provider: item.provider,
                };
                seen.insert(choice.clone()).then_some((choice, label))
            })
            .collect()),
        ConfigField::Thinking => Ok(bounded("读取思考等级", client.thinking_levels(id))
            .await?
            .into_iter()
            .filter(|level| !level.trim().is_empty())
            .filter_map(|level| {
                let label = thinking_label(&level).to_owned();
                let choice = Choice::Thinking(level);
                seen.insert(choice.clone()).then_some((choice, label))
            })
            .collect()),
    }
}

#[derive(Clone)]
pub(crate) enum Evidence {
    Acknowledged,
    Receipt(OperationRef),
    Unknown,
}

pub(crate) enum ResultState {
    Verified,
    Rejected(String),
    Uncertain(Evidence, String),
}

pub(crate) async fn update(
    client: &WorkspaceClient,
    id: &str,
    choice: &Choice,
    evidence: Option<Evidence>,
) -> ResultState {
    if let Some(evidence) = evidence {
        match evidence {
            Evidence::Receipt(operation) => {
                let command = match choice.field() {
                    ConfigField::Model => "set_model",
                    ConfigField::Thinking => "set_thinking_level",
                };
                let receipt = bounded("核对配置回执", client.operation_status(&operation))
                    .await
                    .map(|status| interpret_receipt(status, Some(command), false));
                match receipt {
                    Ok(Some(Ok(()))) => (),
                    Ok(Some(Err(error))) => return ResultState::Rejected(error),
                    Ok(None) => {
                        return ResultState::Uncertain(
                            Evidence::Receipt(operation),
                            "节点尚未确认配置写入结果。".into(),
                        );
                    }
                    Err(error) => {
                        return ResultState::Uncertain(
                            Evidence::Receipt(operation),
                            error.to_string(),
                        );
                    }
                }
            }
            Evidence::Acknowledged => (),
            Evidence::Unknown => {
                return ResultState::Uncertain(
                    evidence,
                    "配置结果未知，节点未返回可核对的回执。".into(),
                );
            }
        }
    } else {
        let result = match choice {
            Choice::Model {
                id: model,
                provider,
            } => {
                bounded(
                    "配置模型",
                    client.set_model_with_provider(id, model, provider),
                )
                .await
            }
            Choice::Thinking(level) => {
                bounded("配置思考等级", client.set_thinking_level(id, level)).await
            }
        };
        match result {
            Ok(_) => (),
            Err(ClientError::OperationOutcomeUnknown { operation }) => {
                return ResultState::Uncertain(
                    Evidence::Receipt(operation),
                    "配置结果未知，可核对节点回执。".into(),
                );
            }
            Err(error) if rejected(&error) => return ResultState::Rejected(error.to_string()),
            Err(error) => return ResultState::Uncertain(Evidence::Unknown, error.to_string()),
        }
    }
    match bounded("核对当前会话配置", client.runtime_state(id)).await {
        Ok(runtime) if choice.matches(&runtime) => ResultState::Verified,
        Ok(_) => ResultState::Uncertain(
            Evidence::Acknowledged,
            "节点已受理，当前配置值有待确认。".into(),
        ),
        Err(error) => ResultState::Uncertain(
            Evidence::Acknowledged,
            format!("节点已受理，读取当前配置失败：{error}"),
        ),
    }
}

#[derive(Default)]
pub(crate) struct Panel {
    pub observed: Option<Observed>,
    observation: Option<Task<()>>,
    menu: Option<Menu>,
    catalog: Option<Task<()>>,
    updates: HashMap<usize, Update>,
}

struct Menu {
    field: ConfigField,
    choices: Option<Result<Choices, String>>,
}

#[derive(Default)]
struct Update {
    task: Option<Task<()>>,
    recovery: Option<(Choice, Evidence)>,
    notice: Option<String>,
}

impl Panel {
    pub fn reset_observation(&mut self) {
        self.close_menu();
        self.observed = None;
        self.observation = None;
    }

    pub fn close_menu(&mut self) -> bool {
        self.catalog = None;
        self.menu.take().is_some()
    }

    pub fn close_if_locked(&mut self, controls: &SessionControls) {
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| !enabled(menu.field, controls))
        {
            self.close_menu();
        }
    }

    pub fn pending(
        &self,
        index: usize,
        requests: &mut work_viewmodel::session_controls::PendingRequests,
    ) {
        if let Some(update) = self.updates.get(&index) {
            requests.configuring |= update.task.is_some();
            requests.uncertain |= update.recovery.is_some();
        }
    }
}

impl Shell {
    pub(super) fn observe_config(&mut self, cx: &mut Context<Self>) {
        self.composer_config.reset_observation();
        let Some((node, frame)) = self.node.as_ref().zip(self.frame.as_ref()) else {
            return;
        };
        if self
            .session
            .and_then(|index| self.composer_config.updates.get(&index))
            .is_some_and(|update| update.task.is_some() || update.recovery.is_some())
            || frame.access.is_none()
            || frame.projection.connection != work_core::ConnectionState::Online
        {
            return;
        }
        let work = node.observe_config(frame.session_id.clone());
        self.composer_config.observation = Some(cx.spawn(async move |this, cx| {
            let observed = work.await.ok().and_then(Result::ok);
            let _ = this.update(cx, |shell, cx| {
                shell.composer_config.observation = None;
                shell.composer_config.observed = observed;
                cx.notify();
            });
        }));
    }

    fn toggle_config(&mut self, field: ConfigField, cx: &mut Context<Self>) {
        if self
            .composer_config
            .menu
            .as_ref()
            .is_some_and(|menu| menu.field == field)
        {
            self.composer_config.close_menu();
            cx.notify();
            return;
        }
        if !enabled(field, &self.session_controls()) {
            return;
        }
        let Some((node, frame)) = self.node.as_ref().zip(self.frame.as_ref()) else {
            return;
        };
        let work = node.config_choices(frame.session_id.clone(), field);
        self.composer_config.menu = Some(Menu {
            field,
            choices: None,
        });
        self.composer_config.catalog = Some(cx.spawn(async move |this, cx| {
            let result = work
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = this.update(cx, |shell, cx| {
                shell.composer_config.catalog = None;
                if let Some(menu) = shell.composer_config.menu.as_mut() {
                    menu.choices = Some(result);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn change_config(&mut self, choice: Choice, cx: &mut Context<Self>) {
        if !enabled(choice.field(), &self.session_controls()) {
            return;
        }
        self.composer_config.close_menu();
        if self
            .composer_config
            .observed
            .as_ref()
            .is_some_and(|observed| choice.matches(&observed.runtime))
        {
            cx.notify();
        } else {
            self.submit_config(choice, None, cx);
        }
    }

    fn check_config(&mut self, cx: &mut Context<Self>) {
        let recovery = self
            .session
            .and_then(|index| self.composer_config.updates.get(&index))
            .filter(|update| update.task.is_none())
            .and_then(|update| update.recovery.clone())
            .filter(|(_, evidence)| !matches!(evidence, Evidence::Unknown));
        if self.session_controls().online
            && let Some((choice, evidence)) = recovery
        {
            self.submit_config(choice, Some(evidence), cx);
        }
    }

    fn submit_config(
        &mut self,
        choice: Choice,
        evidence: Option<Evidence>,
        cx: &mut Context<Self>,
    ) {
        let Some((node, index)) = self.node.as_ref().zip(self.session) else {
            return;
        };
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let work = node.update_config(frame.session_id.clone(), choice.clone(), evidence);
        self.composer_config.reset_observation();
        let update = self.composer_config.updates.entry(index).or_default();
        update.notice = None;
        update.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await.unwrap_or_else(|error| {
                ResultState::Uncertain(Evidence::Unknown, error.to_string())
            });
            let _ = this.update(cx, |shell, cx| {
                let update = shell.composer_config.updates.entry(index).or_default();
                update.task = None;
                update.recovery = None;
                match result {
                    ResultState::Verified => (),
                    ResultState::Rejected(message) => {
                        update.notice = Some(format!("配置被拒绝：{message}"))
                    }
                    ResultState::Uncertain(evidence, message) => {
                        update.recovery = Some((choice, evidence));
                        update.notice = Some(message);
                    }
                }
                if shell.session == Some(index) {
                    shell.observe_config(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn config_notice(&self) -> Option<&str> {
        let update = self.composer_config.updates.get(&self.session?)?;
        if update.task.is_some() {
            Some("正在提交并核对会话配置…")
        } else {
            update.notice.as_deref()
        }
    }

    pub(super) fn config_toolbar(&self, controls: &SessionControls, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let observed = self.composer_config.observed.as_ref();
        let model = observed
            .and_then(|item| item.runtime.model_id.as_deref())
            .unwrap_or("模型");
        let thinking = observed
            .and_then(|item| item.runtime.thinking_level.as_deref())
            .map(thinking_label)
            .unwrap_or("思考等级");
        let recovery = self
            .session
            .and_then(|index| self.composer_config.updates.get(&index))
            .filter(|update| update.task.is_none())
            .and_then(|update| update.recovery.as_ref());
        div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(self.config_control(ConfigField::Model, model, controls, cx))
            .child(self.config_control(ConfigField::Thinking, thinking, controls, cx))
            .when(
                controls.online
                    && recovery.is_some_and(|(_, evidence)| !matches!(evidence, Evidence::Unknown)),
                |bar| {
                    bar.child(
                        self.button("verify-config", "核对配置")
                            .on_click(cx.listener(|shell, _, _, cx| shell.check_config(cx))),
                    )
                },
            )
            .text_color(palette.muted)
    }

    fn config_control(
        &self,
        field: ConfigField,
        label: &str,
        controls: &SessionControls,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let available = enabled(field, controls);
        let id = match field {
            ConfigField::Model => "select-model",
            ConfigField::Thinking => "select-thinking",
        };
        div()
            .relative()
            .flex_none()
            .child(
                self.button(id, "")
                    .gap(px(6.0))
                    .child(
                        self.text(label.to_owned(), 13.0, palette.text)
                            .max_w(px(130.0))
                            .truncate(),
                    )
                    .child(icon("chevron-down", 10.0, palette.muted))
                    .when(available, |button| {
                        button.on_click(
                            cx.listener(move |shell, _, _, cx| shell.toggle_config(field, cx)),
                        )
                    })
                    .when(!available, |button| button.opacity(0.55).cursor_default()),
            )
            .when_some(
                self.composer_config
                    .menu
                    .as_ref()
                    .filter(|menu| available && menu.field == field),
                |control, menu| control.child(deferred(self.config_menu(menu, cx))),
            )
    }

    fn config_menu(&self, menu: &Menu, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let empty = match &menu.choices {
            None => Some("正在读取选项…"),
            Some(Ok(items)) if items.is_empty() => Some("节点未返回可选项"),
            Some(Err(error)) => Some(error.as_str()),
            _ => None,
        };
        let options = menu
            .choices
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .into_iter()
            .flatten()
            .enumerate();
        div()
            .id("config-menu")
            .absolute()
            .left_0()
            .bottom(px(33.0))
            .w(px(246.0))
            .max_h(px(252.0))
            .overflow_y_scroll()
            .occlude()
            .bg(palette.bg)
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.line)
            .shadow_md()
            .p_1()
            .when_some(empty, |menu, text| {
                menu.child(self.text(text.to_owned(), 12.0, palette.muted).p_2())
            })
            .children(options.map(|(index, (choice, label))| {
                let selected = self
                    .composer_config
                    .observed
                    .as_ref()
                    .is_some_and(|current| choice.matches(&current.runtime));
                let choice = choice.clone();
                div()
                    .id(("config-option", index))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .rounded(px(5.0))
                    .px_2()
                    .min_h(px(30.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(palette.hover))
                    .child(
                        self.text(SharedString::from(label.clone()), 13.0, palette.text)
                            .min_w(px(0.0))
                            .truncate(),
                    )
                    .when(selected, |row| {
                        row.child(self.text("✓", 13.0, palette.muted))
                    })
                    .on_click(
                        cx.listener(move |shell, _, _, cx| shell.change_config(choice.clone(), cx)),
                    )
            }))
    }
}

fn enabled(field: ConfigField, controls: &SessionControls) -> bool {
    match field {
        ConfigField::Model => controls.model,
        ConfigField::Thinking => controls.thinking,
    }
}

fn thinking_label(level: &str) -> &str {
    match level {
        "off" => "关闭",
        "minimal" => "极低",
        "low" => "低",
        "medium" => "中",
        "high" => "高",
        "xhigh" => "极高",
        _ => level,
    }
}
