//! wasm-bindgen surface for `work-client`.
//!
//! A Web renderer must not re-implement the node protocol (paths, command
//! envelopes, retries) in TypeScript; that is exactly the duplication the
//! view-model split exists to avoid. This crate exposes the desktop client's
//! read-only surface as promises.
//!
//! Every method returns a JSON string so the JS wrapper stays trivial and the
//! payload shape stays owned by `work-node-api` types.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use futures_util::StreamExt;
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};
use work_client::WorkspaceClient;
use work_client::{MessageSummaryQuery, MessageWindowQuery};

fn json_string<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_json::to_string(value)
        .map(|text| JsValue::from_str(&text))
        .map_err(|_| JsValue::from_str("无法编码节点响应"))
}

fn failure(context: &str, error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{context}：{error}"))
}

/// One authenticated (or pending) connection to a node origin.
#[wasm_bindgen]
pub struct NodeClient {
    client: WorkspaceClient,
    origin: String,
    next_subscription: Rc<Cell<u32>>,
    stops: Rc<RefCell<HashMap<u32, Rc<Cell<bool>>>>>,
}

#[wasm_bindgen]
impl NodeClient {
    #[wasm_bindgen(constructor)]
    pub fn new(origin: &str) -> Result<NodeClient, JsValue> {
        let client =
            WorkspaceClient::new(origin).map_err(|error| failure("节点地址无效", error))?;
        Ok(Self {
            client,
            origin: origin.trim_end_matches('/').to_string(),
            next_subscription: Rc::new(Cell::new(0)),
            stops: Rc::new(RefCell::new(HashMap::new())),
        })
    }

    #[wasm_bindgen(getter)]
    pub fn origin(&self) -> String {
        self.origin.clone()
    }

    /// `{"required":bool,"authenticated":bool}`
    pub fn auth(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let auth = client
                .auth()
                .await
                .map_err(|error| failure("认证状态读取失败", error))?;
            json_string(&serde_json::json!({
                "required": auth.required,
                "authenticated": auth.authenticated,
            }))
        })
    }

    /// Password login for nodes that require one.
    pub fn login(&self, password: String, remember: bool) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let auth = client
                .login(&password)
                .await
                .map_err(|error| failure("登录失败", error))?;
            json_string(&serde_json::json!({
                "required": auth.required,
                "authenticated": auth.authenticated,
                "remembered": remember,
            }))
        })
    }

    /// `[{id,name,path}]`
    pub fn projects(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let projects = client
                .projects()
                .await
                .map_err(|error| failure("项目读取失败", error))?;
            json_string(&serde_json::json!(
                projects
                    .into_iter()
                    .map(|project| serde_json::json!({
                        "id": project.id,
                        "name": project.name,
                        "path": project.path,
                    }))
                    .collect::<Vec<_>>()
            ))
        })
    }

    /// `[{sessionId,projectId,title,status,updatedAt,lastSeq,archived,pinned}]`
    pub fn sessions(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let sessions = client
                .sessions()
                .await
                .map_err(|error| failure("会话读取失败", error))?;
            json_string(&serde_json::json!(
                sessions
                    .into_iter()
                    .map(|session| serde_json::json!({
                        "sessionId": session.session_id,
                        "projectId": session.project_id,
                        "title": session.title,
                        "status": session.status,
                        "updatedAt": session.updated_at,
                        "lastSeq": session.last_seq,
                        "archived": session.archived,
                        "pinned": session.pinned,
                    }))
                    .collect::<Vec<_>>()
            ))
        })
    }

    /// Send a prompt. The reply is the host acknowledgement; the run itself
    /// arrives on the live subscription.
    pub fn prompt(&self, session_id: String, message: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let reply = client
                .prompt(&session_id, &message)
                .await
                .map_err(|error| failure("发送失败", error))?;
            json_string(&serde_json::json!({
                "accepted": reply.success,
                "command": reply.command,
                "data": reply.data,
                "error": reply.error,
            }))
        })
    }

    /// Ask the node to stop the current run.
    pub fn abort(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let reply = client
                .abort(&session_id)
                .await
                .map_err(|error| failure("中止失败", error))?;
            json_string(&serde_json::json!({
                "accepted": reply.success,
                "command": reply.command,
                "data": reply.data,
                "error": reply.error,
            }))
        })
    }

    /// Authoritative runtime state: model, thinking level, context, stats, queue.
    pub fn runtime_state(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let state = client
                .runtime_state(&session_id)
                .await
                .map_err(|error| failure("运行状态读取失败", error))?;
            json_string(&state)
        })
    }

    /// `[{id,name,provider,reasoning,contextWindow}]`
    pub fn models(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let models = client
                .models(&session_id)
                .await
                .map_err(|error| failure("模型列表读取失败", error))?;
            json_string(&serde_json::json!(
                models
                    .into_iter()
                    .map(|model| serde_json::json!({
                        "id": model.id,
                        "name": model.name,
                        "provider": model.provider,
                        "reasoning": model.reasoning,
                        "contextWindow": model.context_window,
                    }))
                    .collect::<Vec<_>>()
            ))
        })
    }

    /// Available thinking levels for the session.
    pub fn thinking_levels(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let levels = client
                .thinking_levels(&session_id)
                .await
                .map_err(|error| failure("思考等级读取失败", error))?;
            json_string(&serde_json::json!(levels))
        })
    }

    /// Switch model, then return the authoritative runtime state.
    pub fn set_model(&self, session_id: String, model_id: String, provider: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            client
                .set_model_with_provider(&session_id, &model_id, &provider)
                .await
                .map_err(|error| failure("模型切换失败", error))?;
            let state = client
                .runtime_state(&session_id)
                .await
                .map_err(|error| failure("切换后回读失败", error))?;
            json_string(&state)
        })
    }

    /// Switch thinking level, then return the authoritative runtime state.
    pub fn set_thinking(&self, session_id: String, level: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            client
                .set_thinking_level(&session_id, &level)
                .await
                .map_err(|error| failure("思考等级切换失败", error))?;
            let state = client
                .runtime_state(&session_id)
                .await
                .map_err(|error| failure("切换后回读失败", error))?;
            json_string(&state)
        })
    }

    /// Directory listing for a project path (`""` is the project root).
    pub fn files(&self, project_id: String, path: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let listing = client
                .files(&project_id, &path)
                .await
                .map_err(|error| failure("目录读取失败", error))?;
            json_string(&listing)
        })
    }

    /// One file's content, bounded by the node.
    pub fn file(&self, project_id: String, path: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let file = client
                .file(&project_id, &path)
                .await
                .map_err(|error| failure("文件读取失败", error))?;
            json_string(&file)
        })
    }

    /// Repository status for a project.
    pub fn git_status(&self, project_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let status = client
                .git_status(&project_id)
                .await
                .map_err(|error| failure("Git 状态读取失败", error))?;
            json_string(&status)
        })
    }

    /// Unified diff for one changed path.
    pub fn git_diff(&self, project_id: String, path: String, staged: bool) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let diff = client
                .diff(&project_id, &path, staged)
                .await
                .map_err(|error| failure("Diff 读取失败", error))?;
            json_string(&diff)
        })
    }

    /// Queued prompts for the session.
    pub fn queue(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let snapshot = client
                .queue(&session_id, false)
                .await
                .map_err(|error| failure("队列读取失败", error))?;
            json_string(&snapshot)
        })
    }

    /// Queue a follow-up prompt instead of interrupting the current run.
    pub fn queue_message(
        &self,
        session_id: String,
        message: String,
        deliver_as: String,
    ) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let outcome = client
                .queue_message(&session_id, &message, &deliver_as)
                .await
                .map_err(|error| failure("加入队列失败", error))?;
            json_string(&outcome)
        })
    }

    /// Cancel one queued prompt; the node answers with the resulting outcome.
    pub fn cancel_queued(&self, session_id: String, entry_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let outcome = client
                .cancel_queued(&session_id, &entry_id)
                .await
                .map_err(|error| failure("取消队列项失败", error))?;
            json_string(&outcome)
        })
    }

    /// Drop every queued prompt for the session.
    pub fn clear_queue(&self, session_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            client
                .clear_queue(&session_id)
                .await
                .map_err(|error| failure("清空队列失败", error))?;
            let snapshot = client
                .queue(&session_id, false)
                .await
                .map_err(|error| failure("队列回读失败", error))?;
            json_string(&snapshot)
        })
    }

    /// Stream live wire events for one session.
    ///
    /// `onEvent` receives one JSON `WireEvent` per call; the returned id can be
    /// passed to [`Self::stop`] to end the subscription. Delivery starts at
    /// `after`, so a reconnect replays only newer events.
    pub fn subscribe(&self, session_id: String, after: f64, on_event: js_sys::Function) -> u32 {
        let client = self.client.clone();
        let id = self.next_subscription.get() + 1;
        self.next_subscription.set(id);
        let stop = Rc::new(Cell::new(false));
        self.stops.borrow_mut().insert(id, stop.clone());
        let stops = self.stops.clone();
        spawn_local(async move {
            let mut stream = match client.subscribe(&session_id, after.max(0.0) as u64).await {
                Ok(stream) => stream,
                Err(error) => {
                    let payload =
                        serde_json::json!({ "error": format!("订阅失败：{error}") }).to_string();
                    let _ = on_event.call1(&JsValue::NULL, &JsValue::from_str(&payload));
                    stops.borrow_mut().remove(&id);
                    return;
                }
            };
            while let Some(item) = stream.next().await {
                if stop.get() {
                    break;
                }
                match item {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event) {
                            let _ = on_event.call1(&JsValue::NULL, &JsValue::from_str(&text));
                        }
                    }
                    Err(error) => {
                        let payload = serde_json::json!({ "error": error.to_string() }).to_string();
                        let _ = on_event.call1(&JsValue::NULL, &JsValue::from_str(&payload));
                        break;
                    }
                }
            }
            stops.borrow_mut().remove(&id);
        });
        id
    }

    /// Stop a live subscription; the stream ends on its next event.
    pub fn stop(&self, id: u32) {
        if let Some(flag) = self.stops.borrow().get(&id) {
            flag.set(true);
        }
    }

    /// Canonical messages for one session.
    ///
    /// Native nodes require exactly one window cursor, so this anchors on the
    /// latest user message: summaries give the anchor, the window gives the
    /// messages around it. Returns `{messages, firstId, lastId, hasMoreBefore}`.
    pub fn messages(&self, session_id: String, limit: u32) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let limit = u64::from(limit.clamp(1, 200));
            let summaries = client
                .message_summaries(
                    &session_id,
                    &MessageSummaryQuery {
                        limit: 10,
                        ..MessageSummaryQuery::default()
                    },
                )
                .await
                .map_err(|error| failure("历史锚点读取失败", error))?;
            let Some(anchor) = summaries
                .messages
                .iter()
                .map(|message| message.entry_id.clone())
                .next_back()
                .filter(|id| !id.is_empty())
            else {
                return json_string(&serde_json::json!({
                    "messages": Vec::<serde_json::Value>::new(),
                    "firstId": null,
                    "lastId": null,
                    "hasMoreBefore": false,
                    "leafId": null,
                }));
            };
            let page = client
                .message_window(
                    &session_id,
                    &MessageWindowQuery {
                        limit,
                        anchor_id: Some(anchor),
                        ..MessageWindowQuery::default()
                    },
                )
                .await
                .map_err(|error| failure("历史读取失败", error))?;
            json_string(&serde_json::json!({
                "messages": page.messages,
                "firstId": page.first_id,
                "lastId": page.last_id,
                "hasMoreBefore": page.has_more_before,
                "leafId": page.leaf_id,
            }))
        })
    }
}
