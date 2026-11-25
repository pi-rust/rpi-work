//! wasm-bindgen surface for `work-client`.
//!
//! A Web renderer must not re-implement the node protocol (paths, command
//! envelopes, retries) in TypeScript; that is exactly the duplication the
//! view-model split exists to avoid. This crate exposes the client's typed
//! surface as promises.
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

mod session;
pub use session::SessionClient;

fn json_string<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_json::to_string(value)
        .map(|text| JsValue::from_str(&text))
        .map_err(|_| JsValue::from_str("无法编码节点响应"))
}

fn failure(context: &str, error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{context}：{error}"))
}

/// Creation has no idempotency key: a transport/server failure may follow a
/// committed write. Keep this distinction in Rust, never parse error text in UI.
#[derive(serde::Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
enum CreationReply<T> {
    Created { value: T },
    Rejected { error: String },
    Unknown { error: String },
}

fn creation_reply<T>(result: Result<T, work_client::ClientError>) -> CreationReply<T> {
    match result {
        Ok(value) => CreationReply::Created { value },
        Err(error) if explicitly_rejected(&error) => CreationReply::Rejected {
            error: error.to_string(),
        },
        Err(error) => CreationReply::Unknown {
            error: error.to_string(),
        },
    }
}

fn explicitly_rejected(error: &work_client::ClientError) -> bool {
    match error {
        work_client::ClientError::Api { status, .. } => {
            matches!(
                status.as_u16(),
                400 | 401 | 403 | 404 | 405 | 409 | 422 | 429
            )
        }
        work_client::ClientError::Unsupported { .. } => true,
        _ => false,
    }
}

fn session_management_reply<T: serde::Serialize>(
    result: Result<T, work_client::ClientError>,
    matches_request: impl FnOnce(&T) -> bool,
) -> Result<JsValue, JsValue> {
    let reply = match result {
        Ok(value) if matches_request(&value) => {
            serde_json::json!({"outcome": "saved", "value": value})
        }
        Ok(_) => serde_json::json!({"outcome": "unknown", "error": "节点回执与会话请求不符"}),
        Err(error) if explicitly_rejected(&error) => {
            serde_json::json!({"outcome": "rejected", "error": error.to_string()})
        }
        Err(error) => serde_json::json!({"outcome": "unknown", "error": error.to_string()}),
    };
    json_string(&reply)
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

    /// `[{sessionId,projectId,title,status,activity,updatedAt,lastSeq,archived,pinned}]`
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
                        "activity": session.activity,
                        "updatedAt": session.updated_at,
                        "lastSeq": session.last_seq,
                        "archived": session.archived,
                        "pinned": session.pinned,
                    }))
                    .collect::<Vec<_>>()
            ))
        })
    }

    pub fn session_management_capabilities(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let capabilities = client
                .capabilities()
                .await
                .map_err(|error| failure("节点能力读取失败", error))?;
            let supports = |name| {
                capabilities
                    .supported_actions
                    .iter()
                    .any(|action| action == name)
            };
            json_string(&serde_json::json!({
                "preferences": supports("session_metadata"),
                "rename": supports("session_organization"),
                "fork": supports("fork"),
            }))
        })
    }

    pub fn set_session_preferences(
        &self,
        session_id: String,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let result = client
                .session_preferences(&session_id, archived, pinned)
                .await;
            session_management_reply(result, |value| {
                value.session_id == session_id
                    && archived.is_none_or(|expected| value.archived == expected)
                    && pinned.is_none_or(|expected| value.pinned == expected)
            })
        })
    }

    pub fn rename_session(&self, session_id: String, title: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let title = title.trim();
            let result = client.rename_session(&session_id, title).await;
            session_management_reply(result, |value| {
                value.session_id == session_id && value.title.as_deref() == Some(title)
            })
        })
    }

    /// Register a directory on the target node; no browser-local path guessing.
    pub fn create_project(&self, name: String, path: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            json_string(&creation_reply(
                client.create_project(&name, Some(&path), false).await,
            ))
        })
    }

    pub fn create_session(&self, project_id: Option<String>) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            json_string(&creation_reply(
                client.create_session(project_id.as_deref()).await,
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

    /// Answer a pending dialog (approval, selector, free-form).
    ///
    /// `values_json` is a JSON array of strings; the envelope shape
    /// (`{values, kind:"selector"}`) is what the node expects for approvals.
    pub fn dialog_response(
        &self,
        session_id: String,
        request_id: String,
        values_json: String,
    ) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let values: Vec<String> = serde_json::from_str(&values_json).unwrap_or_default();
            let answer = serde_json::json!({ "values": values, "kind": "selector" });
            client
                .dialog_response(&session_id, &request_id, answer)
                .await
                .map_err(|error| failure("提交回答失败", error))?;
            Ok(JsValue::from_str("{\"ok\":true}"))
        })
    }

    /// Node identity and version, for the settings page.
    pub fn node_info(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let health = client
                .health()
                .await
                .map_err(|error| failure("节点信息读取失败", error))?;
            let node_id = client.node_identity().await.ok();
            json_string(&serde_json::json!({
                "ok": health.ok,
                "service": health.service,
                "version": health.version,
                "protocol": health.protocol,
                "nodeId": node_id,
            }))
        })
    }

    /// Store an API key for one provider.
    ///
    /// The node answers with a compare-and-swap receipt that already contains
    /// the refreshed catalogue, so the UI never has to guess whether the write
    /// landed.
    pub fn set_provider_key(&self, id: String, key: String, expected_revision: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let receipt = client
                .provider_mutate(work_core::api::ProviderMutation {
                    expected_revision,
                    action: work_core::api::ProviderAction::SetKey { id, key },
                })
                .await
                .map_err(|error| failure("保存密钥失败", error))?;
            json_string(&serde_json::json!({
                "revision": receipt.catalog.revision,
                "providers": receipt.catalog.providers,
                "requiresRestart": receipt.requires_restart,
            }))
        })
    }

    /// One-time authorization for a trusted device (QR payload lives in the
    /// view-model: the envelope is sealed there, not here).
    pub fn issue_qr_authorization(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let authorization = client
                .issue_qr_authorization()
                .await
                .map_err(|error| failure("授权码生成失败", error))?;
            json_string(&authorization)
        })
    }

    /// Provider catalogue with readiness, as the node reports it.
    pub fn provider_catalog(&self) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let catalog = work_core::api::NodeApi::provider_catalog(&client)
                .await
                .map_err(|error| failure("提供商目录读取失败", error))?;
            json_string(&catalog)
        })
    }

    /// Installed native resources (packages, skills, extensions) for a project.
    pub fn native_catalog(&self, project_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let catalog = work_core::api::NodeApi::native_catalog(&client, &project_id)
                .await
                .map_err(|error| failure("资源目录读取失败", error))?;
            json_string(&catalog)
        })
    }

    /// Public package catalog for a project.
    pub fn package_catalog(&self, project_id: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let catalog = work_core::api::NodeApi::package_catalog(&client, &project_id)
                .await
                .map_err(|error| failure("包目录读取失败", error))?;
            json_string(&catalog)
        })
    }

    /// Review bundle for one offer, before anything is acquired or installed.
    pub fn package_bundle(&self, project_id: String, offer_json: String) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let offer: work_core::api::PackageOffer = serde_json::from_str(&offer_json)
                .map_err(|_| JsValue::from_str("资源报价格式无效"))?;
            let reference = work_core::api::PackageRef {
                id: offer.manifest.id.clone(),
                version: offer.manifest.version.clone(),
                sha256: offer.sha256.clone(),
            };
            let bundle = work_core::api::NodeApi::package_bundle(&client, &project_id, reference)
                .await
                .map_err(|error| failure("资源包读取失败", error))?;
            json_string(&bundle)
        })
    }

    /// Acquire, verify, install and re-read one package.
    ///
    /// Mirrors the desktop workflow: the acquisition receipt must match the
    /// requested version and digest, the install is explicit and trusted, and
    /// the catalogue is re-read so the UI never assumes success.
    pub fn install_package(
        &self,
        project_id: String,
        offer_json: String,
        scope: String,
    ) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            let offer: work_core::api::PackageOffer = serde_json::from_str(&offer_json)
                .map_err(|_| JsValue::from_str("资源报价格式无效"))?;
            let reference = work_core::api::PackageRef {
                id: offer.manifest.id.clone(),
                version: offer.manifest.version.clone(),
                sha256: offer.sha256.clone(),
            };
            let scope = match scope.as_str() {
                "project" => work_core::api::NativeScope::Project,
                _ => work_core::api::NativeScope::Global,
            };
            let acquired =
                work_core::api::NodeApi::package_acquire(&client, &project_id, reference.clone())
                    .await
                    .map_err(|error| failure("获取资源失败", error))?;
            if acquired.package != reference {
                return Err(JsValue::from_str("节点获取回执的版本或摘要不匹配"));
            }
            let receipt = work_core::api::NodeApi::package_install(
                &client,
                &project_id,
                work_core::api::PackageInstall {
                    package: reference,
                    scope,
                    trusted: true,
                },
            )
            .await
            .map_err(|error| failure("安装失败", error))?;
            let catalog = work_core::api::NodeApi::native_catalog(&client, &project_id)
                .await
                .map_err(|error| failure("安装后回读失败", error))?;
            json_string(&serde_json::json!({
                "receipt": receipt,
                "catalog": catalog,
            }))
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

    pub fn files_page(&self, project_id: String, path: String, cursor: Option<String>) -> Promise {
        let client = self.client.clone();
        future_to_promise(async move {
            #[derive(serde::Serialize)]
            #[serde(tag = "status", rename_all = "camelCase")]
            enum Read {
                Ready { listing: work_client::FileListing },
                Changed,
            }
            match client
                .files_page(&project_id, &path, cursor.as_deref())
                .await
            {
                Ok(listing) => json_string(&Read::Ready { listing }),
                Err(work_client::ClientError::Api { status, .. })
                    if cursor.is_some() && status.as_u16() == 409 =>
                {
                    json_string(&Read::Changed)
                }
                Err(error) => Err(failure("目录读取失败", error)),
            }
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

#[cfg(test)]
mod tests {
    use super::{CreationReply, creation_reply};
    use work_client::ClientError;

    #[test]
    fn creation_ack_is_an_id_not_a_verified_list() {
        let reply = serde_json::to_value(creation_reply(Ok("node-session-id"))).unwrap();
        assert_eq!(reply["outcome"], "created");
        assert_eq!(reply["value"], "node-session-id");
    }

    #[test]
    fn only_explicit_admission_rejections_allow_another_submission() {
        for status in [400, 401, 403, 404, 405, 409, 422, 429] {
            let reply = creation_reply::<()>(Err(ClientError::Api {
                status: status.try_into().unwrap(),
                message: "rejected".into(),
            }));
            assert!(matches!(reply, CreationReply::Rejected { .. }));
        }
        for status in [408, 500, 502, 504] {
            let reply = creation_reply::<()>(Err(ClientError::Api {
                status: status.try_into().unwrap(),
                message: "may have committed".into(),
            }));
            assert!(matches!(reply, CreationReply::Unknown { .. }));
        }
        assert!(matches!(
            creation_reply::<()>(Err(ClientError::Protocol("malformed reply".into()))),
            CreationReply::Unknown { .. }
        ));
    }
}
