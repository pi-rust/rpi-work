//! Thin lifetime/admission adapter around the canonical client connection.
//! The SDK owns snapshots, cursors, replay, message identity and reconnects.
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

use futures_util::future::{AbortHandle, Abortable};
use js_sys::{Function, Promise};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};
use work_client::{
    ClientError, RuntimeState, SessionAccess, SessionConfiguration, SessionConnection,
    WorkspaceClient, canonical_entry_id,
};
use work_core::{ApplyResult, ConnectionState, RunPhase, RunProjection};
use work_viewmodel::{
    config_requests::{CatalogAccess, bounded},
    session_controls::{
        PendingRequests, SessionControls, SessionObservation, block_wire, controls,
    },
    transcript::{RowSource, TranscriptOrder},
};

use super::{NodeClient, failure, json_string, session_management_reply};

mod actions;
mod recovery;
mod resume;
use recovery::{Intent, Recovery};

struct State {
    id: String,
    projection: RunProjection,
    messages: Vec<Value>,
    dialogs: Vec<work_core::api::PendingDialog>,
    replying: HashSet<String>,
    recovery: Recovery,
    configuration_acknowledged: bool,
    access: Option<SessionAccess>,
    configuration: Option<SessionConfiguration>,
    runtime: Option<RuntimeState>,
    models: Vec<work_core::api::ModelDescriptor>,
    levels: Vec<String>,
    pending: PendingRequests,
    aborting: bool,
    settlements: u64,
    queue_revision: u64,
    pending_resume: bool,
    resume_skipped_at: Option<(u64, u64)>,
    epoch: u64,
    observation_serial: u64,
    closed: Rc<Cell<bool>>,
    error: Option<String>,
    order: TranscriptOrder,
    callback: Function,
}

impl State {
    fn controls(&self) -> SessionControls {
        controls(&SessionObservation {
            projection: (!self.closed.get()).then_some(&self.projection),
            access: self.access.as_ref(),
            configuration: self.configuration.as_ref(),
            runtime: self.runtime.as_ref(),
            has_dialogs: !self.dialogs.is_empty(),
            pending_resume: self.pending_resume && !self.resume_was_skipped(),
            pending: self.pending,
        })
    }

    fn fork_available(&self) -> bool {
        let controls = self.controls();
        controls.online
            && !controls.busy
            && !self.pending.configuring
            && !self.pending.uncertain
            && self.dialogs.is_empty()
            && self
                .access
                .as_ref()
                .is_some_and(|access| access.descriptor.identity.protocol == "rpi")
    }

    fn payload(&mut self) -> Value {
        let ordered = self
            .order
            .update(&self.id, &self.messages, &self.projection.blocks);
        let transcript = work_viewmodel::conversation::transcript_view::present(
            &ordered,
            &self.messages,
            &self.projection.blocks,
            Some(&self.projection.phase),
            Some(&self.projection.connection),
            self.pending,
        );
        let rows: Vec<_> = ordered
            .into_iter()
            .map(|row| {
                let (source, index) = match row.source {
                    RowSource::History(index) => ("history", index),
                    RowSource::Projection(index) => ("projection", index),
                };
                json!({"key":row.key,"source":source,"index":index})
            })
            .collect();
        json!({
            "scopeEpoch": self.epoch,
            "queueRevision": self.queue_revision,
            "recovery": self.recovery.view(self.controls().online),
            "messages": self.messages,
            "blocks": self.projection.blocks.iter().map(block_wire).collect::<Vec<_>>(),
            "rows": rows,
            "transcript": transcript,
            "phase": format!("{:?}", self.projection.phase),
            "settled": matches!(self.projection.phase, RunPhase::Settled(_)),
            "controls": self.controls(),
            "forkAvailable": self.fork_available(),
            "forkFromEntry": self.access.as_ref().is_some_and(|access| access.supports_command("navigate")),
            "dialogs": self.dialogs.iter().map(|dialog| json!({
                "requestId":dialog.request_id, "purpose":dialog.ui.purpose,
                "replyEnabled":self.controls().reply && !self.replying.contains(&dialog.request_id),
                "question":dialog.ui.question.as_ref().or(dialog.ui.message.as_ref()),
                "options":dialog.ui.options.iter().map(|option| json!({
                    "label":option.title.as_deref().or_else(|| option.value.as_ref().and_then(Value::as_str)).unwrap_or("未命名选项"),
                    "value":option.value.as_ref().and_then(Value::as_str),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "runtime": self.runtime,
            "models": self.models.iter().map(|model| json!({
                "id":model.id,"provider":model.provider,"name":model.name,
                "reasoning":model.reasoning,"contextWindow":model.context_window,
            })).collect::<Vec<_>>(),
            "thinkingLevels": self.levels,
            "error": self.error,
        })
    }

    fn publish(&mut self) {
        if !self.closed.get() {
            let text = self.payload().to_string();
            let callback = self.callback.clone();
            let closed = self.closed.clone();
            // Never invoke renderer code while holding a RefCell borrow.
            spawn_local(async move {
                if !closed.get() {
                    let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&text));
                }
            });
        }
    }

    fn sync(&mut self, connection: &SessionConnection, kind: &str, restore_active: &mut bool) {
        let restoring = matches!(kind, "connection_reconnecting" | "resync_required");
        if restoring {
            *restore_active |= (self.pending.submitting
                || self.pending.awaiting_reply
                || self.pending.cancelling
                || self.recovery.has_pending_resume())
                && !matches!(
                    self.projection.phase,
                    RunPhase::Ready | RunPhase::Settled(_)
                );
        }
        self.projection = connection.projection().clone();
        self.messages = connection.messages().to_vec();
        self.dialogs = connection.pending_dialogs().to_vec();
        self.pending_resume = connection.pending_resume();
        self.replying
            .retain(|id| self.dialogs.iter().any(|dialog| &dialog.request_id == id));
        if restoring {
            self.epoch = self.epoch.wrapping_add(1);
            self.access = None;
            self.configuration = None;
            self.runtime = None;
            self.models.clear();
            self.levels.clear();
        }
        // Only the SDK's fresh terminal projection following observed activity
        // can close a restored acceptance window. Ready or an unstarted request
        // with a previous run's terminal projection is not new settlement.
        let restored_terminal = kind == "connection_online"
            && *restore_active
            && matches!(self.projection.phase, RunPhase::Settled(_));
        if kind == "agent_settled" || restored_terminal {
            self.settlements = self.settlements.wrapping_add(1);
            self.queue_revision = self.queue_revision.wrapping_add(1);
            if !self.pending.submitting {
                self.pending.awaiting_reply = false;
            }
            if !self.aborting {
                self.pending.cancelling = false;
            }
        }
        if kind == "connection_online" {
            *restore_active = false;
        }
        self.publish();
    }
}

/// One observed session; dropping/closing this observer never stops its owner.
#[wasm_bindgen]
pub struct SessionClient {
    client: WorkspaceClient,
    id: String,
    state: Rc<RefCell<State>>,
    stop: AbortHandle,
}

#[wasm_bindgen]
impl NodeClient {
    /// SDK open + exact access + coherent snapshot/stream, not a second owner.
    pub fn bind_session(&self, id: String, callback: Function) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let access = client
                .session_access(&id)
                .await
                .map_err(|error| failure("会话能力读取失败", error))?;
            let mut connection = client
                .connect_session_with_access(id.clone(), &access)
                .await
                .map_err(|error| failure("会话同步失败", error))?;
            let state = Rc::new(RefCell::new(State {
                id: id.clone(),
                projection: connection.projection().clone(),
                messages: connection.messages().to_vec(),
                dialogs: connection.pending_dialogs().to_vec(),
                replying: HashSet::new(),
                recovery: Recovery::default(),
                configuration_acknowledged: false,
                access: Some(access),
                configuration: None,
                runtime: None,
                models: vec![],
                levels: vec![],
                pending: PendingRequests::default(),
                aborting: false,
                settlements: 0,
                queue_revision: 0,
                pending_resume: connection.pending_resume(),
                resume_skipped_at: None,
                epoch: 0,
                observation_serial: 0,
                closed: Rc::new(Cell::new(false)),
                error: None,
                order: TranscriptOrder::default(),
                callback,
            }));
            state.borrow_mut().publish();
            observe(client.clone(), id.clone(), state.clone());
            let (stop, registration) = AbortHandle::new_pair();
            let observed = state.clone();
            let live_client = client.clone();
            let live_id = id.clone();
            spawn_local(async move {
                let _ = Abortable::new(
                    async move {
                        let mut restore_active = false;
                        loop {
                            let result = connection.next().await;
                            if observed.borrow().closed.get() {
                                break;
                            }
                            let event = match result {
                                Ok(Some(event)) => event,
                                Ok(None) => break,
                                Err(error) => {
                                    let mut state = observed.borrow_mut();
                                    state.projection.connection = ConnectionState::Offline;
                                    state.access = None;
                                    state.error = Some(error.to_string());
                                    state.publish();
                                    break;
                                }
                            };
                            if matches!(
                                event.applied,
                                ApplyResult::IgnoredStale | ApplyResult::IgnoredDuplicate
                            ) {
                                continue;
                            }
                            let kind = event.wire.event_type.as_str();
                            let refresh = matches!(kind, "connection_online" | "agent_settled");
                            observed
                                .borrow_mut()
                                .sync(&connection, kind, &mut restore_active);
                            if refresh {
                                observe(live_client.clone(), live_id.clone(), observed.clone());
                            }
                        }
                    },
                    registration,
                )
                .await;
            });
            Ok(SessionClient {
                client,
                id,
                state,
                stop,
            }
            .into())
        })
    }
}

fn observe(client: WorkspaceClient, id: String, observed: Rc<RefCell<State>>) {
    let (epoch, serial) = {
        let mut state = observed.borrow_mut();
        state.observation_serial = state.observation_serial.wrapping_add(1);
        (state.epoch, state.observation_serial)
    };
    spawn_local(async move {
        let mut observed_access = None;
        let result = async {
            let access = bounded("读取会话能力", client.session_access(&id)).await?;
            observed_access = Some(access.clone());
            let config = bounded("读取配置能力", client.session_configuration(&id)).await?;
            let catalog = CatalogAccess::for_configuration(&config);
            let runtime = if access.supports_command("get_state") {
                bounded("读取运行观察", client.runtime_state(&id))
                    .await
                    .ok()
            } else {
                None
            };
            let models = if catalog.models_read {
                bounded("读取模型目录", client.models(&id))
                    .await
                    .unwrap_or_default()
            } else {
                vec![]
            };
            let levels = if catalog.thinking_read {
                bounded("读取思考目录", client.thinking_levels(&id))
                    .await
                    .unwrap_or_default()
            } else {
                vec![]
            };
            Ok::<_, ClientError>((access, config, runtime, models, levels))
        }
        .await;
        let mut state = observed.borrow_mut();
        if state.closed.get() || state.epoch != epoch || state.observation_serial != serial {
            return;
        }
        match result {
            Ok((access, config, runtime, models, levels)) => {
                state.access = Some(access);
                state.configuration = Some(config);
                state.runtime = runtime;
                state.models = models;
                state.levels = levels;
            }
            Err(error) => {
                // Configuration is optional for sending, but a failed access
                // read cannot retain an earlier binding's write affordances.
                state.access = observed_access;
                state.configuration = None;
                state.error = Some(error.to_string());
            }
        }
        state.publish();
    });
}

fn rejected(error: &ClientError) -> bool {
    matches!(error, ClientError::Unsupported { .. })
        || matches!(error, ClientError::Api { status, .. }
        if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 405 | 409 | 422 | 429))
}

#[wasm_bindgen]
impl SessionClient {
    pub fn view(&self) -> String {
        self.state.borrow_mut().payload().to_string()
    }

    pub fn close(&self) {
        self.state.borrow().closed.set(true);
        self.stop.abort();
    }

    pub fn fork(&self, entry_id: Option<String>, source_title: String) -> Promise {
        let client = self.client.clone();
        let id = self.id.clone();
        let observed = self.state.clone();
        future_to_promise(async move {
            let allowed = {
                let state = observed.borrow();
                state.fork_available()
                    && entry_id.as_deref().is_none_or(|entry| {
                        state
                            .access
                            .as_ref()
                            .is_some_and(|access| access.supports_command("navigate"))
                            && state.messages.iter().any(|message| {
                                message["role"] == "assistant"
                                    && canonical_entry_id(message) == Some(entry)
                            })
                    })
            };
            if !allowed {
                return json_string(
                    &json!({"outcome": "rejected", "error": "当前会话或历史消息暂不能分叉"}),
                );
            }
            let suffix = "（分叉）";
            let base: String = source_title
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(120 - suffix.chars().count())
                .collect();
            let title = format!("{}{suffix}", base.trim_end());
            let result = client
                .fork_session(&id, entry_id.as_deref(), Some(&title))
                .await;
            session_management_reply(result, |value| {
                !value.session_id.trim().is_empty()
                    && value.session_id != id
                    && value.source_session_id.as_deref() == Some(id.as_str())
                    && value.entry_id == entry_id
                    && value.title.as_deref() == Some(title.as_str())
            })
        })
    }

    /// Accepted is only admission; settlement continues through SessionConnection.
    pub fn prompt(&self, message: String) -> Promise {
        let client = self.client.clone();
        let id = self.id.clone();
        let observed = self.state.clone();
        future_to_promise(async move {
            let (stamp, epoch) = {
                let mut state = observed.borrow_mut();
                if !state.controls().send || message.trim().is_empty() {
                    return Err(JsValue::from_str("当前绑定会话不允许发送"));
                }
                state.pending.submitting = true;
                state.recovery.clear_message();
                state.error = None;
                state.publish();
                (state.settlements, state.epoch)
            };
            let result = client.prompt(&id, &message).await;
            let mut state = observed.borrow_mut();
            state.pending.submitting = false;
            let response = match result {
                Ok(reply) => {
                    let accepted = reply.success
                        && reply
                            .data
                            .as_ref()
                            .and_then(|data| data.get("accepted"))
                            .and_then(Value::as_bool)
                            != Some(false);
                    state.pending.awaiting_reply = accepted && state.settlements == stamp;
                    if !accepted {
                        state.error = Some(reply.error.unwrap_or_else(|| "节点拒绝发送".into()));
                    }
                    json!({"accepted":accepted,"error":state.error})
                }
                Err(error) => {
                    if !rejected(&error) {
                        state.remember_unknown(Some(&error), Intent::Prompt, stamp);
                    }
                    state.error = Some(if state.epoch == epoch {
                        error.to_string()
                    } else {
                        "上次观察范围的发送未确认；草稿保留，不会自动重发".into()
                    });
                    json!({"accepted":false,"error":state.error,"unknown":state.pending.uncertain})
                }
            };
            state.publish();
            json_string(&response)
        })
    }

    pub fn abort(&self) -> Promise {
        let client = self.client.clone();
        let id = self.id.clone();
        let observed = self.state.clone();
        future_to_promise(async move {
            let (stamp, epoch) = {
                let mut state = observed.borrow_mut();
                if !state.controls().abort {
                    return Err(JsValue::from_str("当前绑定会话不允许停止"));
                }
                state.pending.cancelling = true;
                state.recovery.clear_message();
                state.aborting = true;
                state.publish();
                (state.settlements, state.epoch)
            };
            let result = client.abort(&id).await;
            let mut state = observed.borrow_mut();
            state.aborting = false;
            match result {
                Ok(reply) => {
                    let accepted = reply.success
                        && reply
                            .data
                            .as_ref()
                            .and_then(|data| data.get("accepted"))
                            .and_then(Value::as_bool)
                            != Some(false);
                    state.pending.cancelling = accepted && state.settlements == stamp;
                    if !accepted {
                        state.error = Some(reply.error.unwrap_or_else(|| "节点拒绝停止".into()));
                    }
                }
                Err(error) => {
                    if !rejected(&error) {
                        state.remember_unknown(Some(&error), Intent::Abort, stamp);
                    }
                    state.pending.cancelling = !rejected(&error);
                    state.error = Some(if state.epoch == epoch {
                        error.to_string()
                    } else {
                        "上次观察范围的停止请求未确认；不能据此宣告已停止".into()
                    });
                }
            }
            state.publish();
            json_string(&json!({"error":state.error}))
        })
    }
}

impl Drop for SessionClient {
    fn drop(&mut self) {
        self.close();
    }
}
