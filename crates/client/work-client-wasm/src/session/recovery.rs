//! Read-only recovery of local request receipts, never an execution owner.
use js_sys::Promise;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use work_client::{ClientError, RuntimeState};
use work_core::api::realtime::OperationRef;
use work_viewmodel::{config_requests::bounded, session_receipts::interpret_receipt};

use super::{SessionClient, State, json_string};

#[derive(Clone)]
pub(super) enum Intent {
    Prompt,
    Abort,
    Model { id: String, provider: String },
    Thinking(String),
    Reply(String),
    Queue,
    Other,
}

impl Intent {
    pub(super) fn configuration(&self) -> bool {
        matches!(self, Self::Model { .. } | Self::Thinking(_))
    }
    fn command(&self) -> Option<&str> {
        match self {
            Self::Prompt => Some("prompt"),
            Self::Abort => Some("abort"),
            Self::Model { .. } => Some("set_model"),
            Self::Thinking(_) => Some("set_thinking_level"),
            Self::Queue => Some("queue_message"),
            _ => None,
        }
    }
    fn matches_runtime(&self, runtime: &RuntimeState) -> bool {
        match self {
            Self::Model { id, provider } => {
                runtime.model_id.as_ref() == Some(id) && runtime.provider.as_ref() == Some(provider)
            }
            Self::Thinking(level) => runtime.thinking_level.as_ref() == Some(level),
            _ => false,
        }
    }
}

#[derive(Clone)]
enum Evidence {
    Receipt(OperationRef),
    // Not inferred from runtime: the actual setter future already returned Ok.
    ConfigurationAck,
}
#[derive(Clone)]
struct Entry {
    id: u64,
    evidence: Evidence,
    intent: Intent,
    settlements: u64,
}
#[derive(Default)]
pub(super) struct Recovery {
    entries: Vec<Entry>,
    next: u64,
    unreceipted: bool,
    checking: bool,
    message: Option<String>,
}
impl Recovery {
    pub(super) fn view(&self, online: bool) -> Value {
        json!({"canCheck": online && !self.checking && !self.entries.is_empty(), "checking": self.checking,
            "count": self.entries.len(), "unreceipted": self.unreceipted, "message": self.message})
    }
    pub(super) fn clear_message(&mut self) {
        self.message = None;
    }
    fn locked(&self) -> bool {
        self.unreceipted || !self.entries.is_empty()
    }
    fn record(
        &mut self,
        error: Option<&ClientError>,
        intent: Intent,
        settlements: u64,
        acknowledged: bool,
    ) {
        let evidence = match error {
            Some(ClientError::OperationOutcomeUnknown { operation }) => {
                Evidence::Receipt(operation.clone())
            }
            _ if intent.configuration() && acknowledged => Evidence::ConfigurationAck,
            _ => {
                self.unreceipted = true;
                return;
            }
        };
        if let Evidence::Receipt(operation) = &evidence
            && self
                .entries
                .iter()
                .any(|entry| matches!(&entry.evidence, Evidence::Receipt(old) if old == operation))
        {
            return;
        }
        if self.entries.len() >= 64 {
            self.unreceipted = true;
            return;
        }
        self.next = self.next.wrapping_add(1);
        self.entries.push(Entry {
            id: self.next,
            evidence,
            intent,
            settlements,
        });
    }
}

impl State {
    pub(super) fn remember_unknown(
        &mut self,
        error: Option<&ClientError>,
        intent: Intent,
        stamp: u64,
    ) {
        self.recovery
            .record(error, intent, stamp, self.configuration_acknowledged);
        self.pending.uncertain = self.recovery.locked();
    }
}

#[wasm_bindgen]
impl SessionClient {
    /// Only GET receipt status and read current runtime. Never repeat a mutation.
    pub fn recheck(&self) -> Promise {
        let observed = self.state.clone();
        let client = self.client.clone();
        let id = self.id.clone();
        future_to_promise(async move {
            let (entries, epoch, serial) = {
                let mut state = observed.borrow_mut();
                if state.closed.get()
                    || !state.controls().online
                    || state.recovery.checking
                    || state.recovery.entries.is_empty()
                {
                    return Err(JsValue::from_str("没有可只读核对的回执，未自动重发"));
                }
                state.recovery.checking = true;
                state.observation_serial = state.observation_serial.wrapping_add(1);
                state.publish();
                (
                    state.recovery.entries.clone(),
                    state.epoch,
                    state.observation_serial,
                )
            };
            let _ = bounded("只读核对整个请求账本", async {
                for entry in entries {
                    let result = match &entry.evidence {
                        Evidence::Receipt(operation) => {
                            match bounded("只读核对操作回执", client.operation_status(operation))
                                .await
                            {
                                Ok(status) => interpret_receipt(
                                    status,
                                    entry.intent.command(),
                                    matches!(entry.intent, Intent::Queue | Intent::Reply(_)),
                                ),
                                Err(_) => None,
                            }
                        }
                        Evidence::ConfigurationAck => Some(Ok(())),
                    };
                    let runtime = if matches!(result, Some(Ok(()))) && entry.intent.configuration()
                    {
                        bounded("只读核对配置效果", client.runtime_state(&id))
                            .await
                            .ok()
                            .filter(|runtime| entry.intent.matches_runtime(runtime))
                    } else {
                        None
                    };
                    let mut state = observed.borrow_mut();
                    if state.closed.get() || state.epoch != epoch {
                        break;
                    }
                    let Some(result) = result else {
                        continue;
                    };
                    if result.is_ok()
                        && entry.intent.configuration()
                        && (runtime.is_none() || state.observation_serial != serial)
                    {
                        continue;
                    }
                    state
                        .recovery
                        .entries
                        .retain(|pending| pending.id != entry.id);
                    match result {
                        Ok(()) => match &entry.intent {
                            Intent::Prompt => {
                                state.pending.awaiting_reply =
                                    state.settlements == entry.settlements
                            }
                            Intent::Abort => {
                                state.pending.cancelling = state.settlements == entry.settlements
                            }
                            Intent::Model { .. } | Intent::Thinking(_) => {
                                state.runtime = runtime;
                                state.pending.configuring = false;
                            }
                            _ => {}
                        },
                        Err(error) => {
                            if let Intent::Reply(request) = &entry.intent {
                                state.replying.remove(request);
                            }
                            if entry.intent.configuration() {
                                state.pending.configuring = false;
                            }
                            if matches!(entry.intent, Intent::Abort) {
                                state.pending.cancelling = false;
                            }
                            state.error = Some(format!("原请求回执明确拒绝: {error}"));
                        }
                    }
                }
                Ok::<(), ClientError>(())
            })
            .await;
            let mut state = observed.borrow_mut();
            state.recovery.checking = false;
            state.pending.uncertain = state.recovery.locked();
            if !state.closed.get() && state.epoch == epoch {
                state.recovery.message = Some(
                    if state.pending.uncertain {
                        "仍有请求结果或效果未核实，继续锁定；不会自动重发。"
                    } else {
                        "只读核对完成；受理不等于运行结算，保留的草稿未自动发送。"
                    }
                    .into(),
                );
                if !state.pending.uncertain
                    && !state
                        .error
                        .as_deref()
                        .is_some_and(|error| error.starts_with("原请求回执明确拒绝:"))
                {
                    state.error = None;
                }
            }
            if !state.closed.get() {
                state.publish();
            }
            let refresh = !state.closed.get() && state.epoch == epoch;
            let payload = state.recovery.view(state.controls().online);
            drop(state);
            // Recovery may supersede an in-flight metadata observation. Start a
            // fresh canonical read, without clearing any unresolved request.
            if refresh {
                super::observe(client, id, observed);
            }
            json_string(&payload)
        })
    }
}
