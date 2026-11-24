//! Typed actions on the observed session; admission stays in shared Rust rules.
use std::future::Future;

use js_sys::Promise;
use serde::Serialize;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use work_client::{ClientError, RuntimeState, WorkspaceClient};
use work_viewmodel::config_requests::bounded;

use super::{Intent, SessionClient, State, failure, json_string, rejected};

enum Action {
    QueueRead,
    Queue,
    CancelQueued,
    ClearQueue,
    Reply {
        request_id: String,
        values: Vec<String>,
    },
    Model {
        id: String,
        provider: String,
    },
    Thinking(String),
}

impl Action {
    fn allowed(&self, state: &State) -> bool {
        let view = state.controls();
        match self {
            Self::QueueRead => view.queue_read,
            Self::Queue => view.queue,
            Self::CancelQueued => view.cancel_queued,
            Self::ClearQueue => view.clear_queue,
            Self::Reply { request_id, values } => {
                view.reply
                    && values.len() == 1
                    && !state.replying.contains(request_id)
                    && state.dialogs.iter().any(|dialog| {
                        &dialog.request_id == request_id
                            && dialog.ui.options.iter().any(|option| {
                                option.value.as_ref().and_then(Value::as_str)
                                    == Some(values[0].as_str())
                            })
                    })
            }
            Self::Model { id, provider } => {
                view.model
                    && state
                        .models
                        .iter()
                        .any(|model| &model.id == id && &model.provider == provider)
            }
            Self::Thinking(level) => view.thinking && state.levels.contains(level),
        }
    }
    fn configuration(&self) -> bool {
        matches!(self, Self::Model { .. } | Self::Thinking(_))
    }
    fn intent(&self) -> Intent {
        match self {
            Self::Model { id, provider } => Intent::Model {
                id: id.clone(),
                provider: provider.clone(),
            },
            Self::Thinking(level) => Intent::Thinking(level.clone()),
            Self::Reply { request_id, .. } => Intent::Reply(request_id.clone()),
            Self::Queue => Intent::Queue,
            _ => Intent::Other,
        }
    }
    fn queue_unknown(&self, value: &impl Serialize) -> bool {
        matches!(self, Self::Queue)
            && serde_json::to_value(value)
                .ok()
                .as_ref()
                .and_then(|value| value.get("accepted"))
                .and_then(Value::as_bool)
                .is_none()
    }
    fn failed(&self, state: &mut State, error: &ClientError, stamp: u64) {
        let unknown = !matches!(self, Self::QueueRead) && !rejected(error);
        if unknown {
            state.remember_unknown(Some(error), self.intent(), stamp);
        }
        if unknown && self.configuration() {
            state.runtime = None;
        }
        if !unknown && let Self::Reply { request_id, .. } = self {
            state.replying.remove(request_id);
        }
        if self.configuration() {
            state.pending.configuring = unknown;
        }
    }
}

impl SessionClient {
    fn request<T, F>(
        &self,
        action: Action,
        request: impl FnOnce(WorkspaceClient, String) -> F + 'static,
        update: fn(&mut State, &T),
    ) -> Promise
    where
        T: Serialize + 'static,
        F: Future<Output = Result<T, ClientError>> + 'static,
    {
        let observed = self.state.clone();
        let client = self.client.clone();
        let id = self.id.clone();
        future_to_promise(async move {
            let (epoch, stamp) = {
                let mut state = observed.borrow_mut();
                if !action.allowed(&state) {
                    return Err(JsValue::from_str("当前绑定会话不允许此操作"));
                }
                if let Action::Reply { request_id, .. } = &action {
                    state.replying.insert(request_id.clone());
                }
                if action.configuration() {
                    state.observation_serial = state.observation_serial.wrapping_add(1);
                    state.pending.configuring = true;
                    state.configuration_acknowledged = false;
                }
                if !matches!(action, Action::QueueRead) {
                    state.recovery.clear_message();
                    state.error = None;
                }
                state.publish();
                (state.epoch, state.settlements)
            };
            let result = request(client, id).await;
            let mut state = observed.borrow_mut();
            if state.closed.get() {
                return Err(JsValue::from_str("会话观察已关闭"));
            }
            match result {
                Ok(value) => {
                    if state.epoch != epoch {
                        if action.configuration() || action.queue_unknown(&value) {
                            state.remember_unknown(None, action.intent(), stamp);
                        }
                        state.publish();
                        return Err(JsValue::from_str(
                            "观察范围已变更；旧结果未替换当前观察，请只读核对",
                        ));
                    }
                    if action.configuration() {
                        state.pending.configuring = false;
                    }
                    if action.queue_unknown(&value) {
                        state.remember_unknown(None, action.intent(), stamp);
                        state.error = Some("队列受理结果未知，草稿保留；不能自动重发".into());
                    }
                    update(&mut state, &value);
                    state.publish();
                    json_string(&value)
                }
                Err(error) => {
                    action.failed(&mut state, &error, stamp);
                    state.error = Some(if state.epoch == epoch {
                        error.to_string()
                    } else {
                        "上次观察范围的请求未确认；旧数据未替换当前观察，请只读核对".into()
                    });
                    state.publish();
                    Err(failure("会话操作失败", error))
                }
            }
        })
    }
}

#[wasm_bindgen]
impl SessionClient {
    pub fn queue(&self) -> Promise {
        self.request(
            Action::QueueRead,
            |client, id| async move { client.queue(&id, false).await },
            |_, _| {},
        )
    }
    pub fn queue_message(&self, text: String, deliver_as: String) -> Promise {
        self.request(
            Action::Queue,
            move |client, id| async move { client.queue_message(&id, &text, &deliver_as).await },
            |_, _| {},
        )
    }
    pub fn cancel_queued(&self, entry: String) -> Promise {
        self.request(
            Action::CancelQueued,
            |client, id| async move { client.cancel_queued(&id, &entry).await },
            |_, _| {},
        )
    }
    pub fn clear_queue(&self) -> Promise {
        self.request(
            Action::ClearQueue,
            |client, id| async move { client.clear_queue(&id).await },
            |_, _| {},
        )
    }
    pub fn dialog_response(&self, request_id: String, values: String) -> Promise {
        let values: Vec<String> = match serde_json::from_str(&values) {
            Ok(values) => values,
            Err(_) => return Promise::reject(&JsValue::from_str("仅支持真实字符串选项的单项回复")),
        };
        self.request(
            Action::Reply {
                request_id: request_id.clone(),
                values: values.clone(),
            },
            move |client, id| async move {
                client
                    .dialog_response(&id, &request_id, json!({"values":values,"kind":"selector"}))
                    .await
            },
            |_, _| {},
        )
    }
    pub fn set_model(&self, model: String, provider: String) -> Promise {
        let acknowledged = self.state.clone();
        self.request(
            Action::Model {
                id: model.clone(),
                provider: provider.clone(),
            },
            move |client, id| async move {
                bounded("配置模型", async {
                    client
                        .set_model_with_provider(&id, &model, &provider)
                        .await?;
                    acknowledged.borrow_mut().configuration_acknowledged = true;
                    let runtime = client.runtime_state(&id).await.map_err(|error| {
                        ClientError::Protocol(format!("模型写入已受理，但权威回读失败: {error}"))
                    })?;
                    if runtime.model_id.as_deref() != Some(model.as_str())
                        || runtime.provider.as_deref() != Some(provider.as_str())
                    {
                        return Err(ClientError::Protocol(
                            "模型写入尚未取得精确权威回读，不能视为生效".into(),
                        ));
                    }
                    Ok(runtime)
                })
                .await
            },
            |state, runtime: &RuntimeState| state.runtime = Some(runtime.clone()),
        )
    }
    pub fn set_thinking(&self, level: String) -> Promise {
        let acknowledged = self.state.clone();
        self.request(
            Action::Thinking(level.clone()),
            move |client, id| async move {
                bounded("配置思考等级", async {
                    client.set_thinking_level(&id, &level).await?;
                    acknowledged.borrow_mut().configuration_acknowledged = true;
                    let runtime = client.runtime_state(&id).await.map_err(|error| {
                        ClientError::Protocol(format!(
                            "思考等级写入已受理，但权威回读失败: {error}"
                        ))
                    })?;
                    if runtime.thinking_level.as_deref() != Some(level.as_str()) {
                        return Err(ClientError::Protocol(
                            "思考等级尚未取得权威回读，不能视为生效".into(),
                        ));
                    }
                    Ok(runtime)
                })
                .await
            },
            |state, runtime: &RuntimeState| state.runtime = Some(runtime.clone()),
        )
    }
}
