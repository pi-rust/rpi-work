//! wasm-bindgen surface for `work-client`.
//!
//! A Web renderer must not re-implement the node protocol (paths, command
//! envelopes, retries) in TypeScript; that is exactly the duplication the
//! view-model split exists to avoid. This crate exposes the desktop client's
//! read-only surface as promises.
//!
//! Every method returns a JSON string so the JS wrapper stays trivial and the
//! payload shape stays owned by `work-node-api` types.

use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
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
