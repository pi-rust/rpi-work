//! Admission of node-reported pending work on the observed session.
use js_sys::Promise;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use work_viewmodel::session_receipts::{ResumeAdmission, resume_admission};

use super::{Intent, SessionClient, State, rejected};

impl State {
    pub(super) fn resume_was_skipped(&self) -> bool {
        self.resume_skipped_at == Some((self.epoch, self.settlements))
    }

    pub(super) fn observe_resume_ack(
        &mut self,
        admission: ResumeAdmission,
        epoch: u64,
        stamp: u64,
    ) {
        match admission {
            ResumeAdmission::Accepted => {
                self.pending.awaiting_reply = self.settlements == stamp;
            }
            ResumeAdmission::Skipped if self.epoch == epoch && self.settlements == stamp => {
                self.resume_skipped_at = Some((epoch, stamp));
                self.queue_revision = self.queue_revision.wrapping_add(1);
                self.error = Some("节点确认没有待恢复的运行".into());
            }
            ResumeAdmission::Skipped => {}
        }
    }
}

#[wasm_bindgen]
impl SessionClient {
    pub fn resume_pending(&self) -> Promise {
        let client = self.client.clone();
        let id = self.id.clone();
        let observed = self.state.clone();
        future_to_promise(async move {
            let (epoch, stamp) = {
                let mut state = observed.borrow_mut();
                if state.controls().resume_pending != Some(true) {
                    return Err(JsValue::from_str("当前绑定会话不能继续队列"));
                }
                state.pending.submitting = true;
                state.error = None;
                state.recovery.clear_message();
                state.publish();
                (state.epoch, state.settlements)
            };
            let result = client
                .command::<Value>(&id, json!({"type":"resume_pending"}))
                .await;
            let mut state = observed.borrow_mut();
            if state.closed.get() {
                return Err(JsValue::from_str("会话观察已关闭"));
            }
            state.pending.submitting = false;
            let admission = match result {
                Ok(reply) => resume_admission(reply),
                Err(error) if rejected(&error) => Some(Err(error.to_string())),
                Err(error) => {
                    state.remember_unknown(Some(&error), Intent::Resume { epoch }, stamp);
                    state.error = Some(error.to_string());
                    state.publish();
                    return Ok(JsValue::UNDEFINED);
                }
            };
            match admission {
                Some(Ok(admission)) => state.observe_resume_ack(admission, epoch, stamp),
                Some(Err(error)) => state.error = Some(error),
                None => {
                    state.remember_unknown(None, Intent::Resume { epoch }, stamp);
                    state.error = Some("继续队列的受理结果未知".into());
                }
            }
            state.publish();
            Ok(JsValue::UNDEFINED)
        })
    }
}
