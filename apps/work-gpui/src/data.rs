//! Workbench data from an authenticated Work node or the layout sample.

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};
use work_client::{
    ClientError, Project, ProjectFolders, SessionAccess, SessionConnection, WorkspaceClient,
};
use work_core::api::realtime::{OperationRef, OperationStatus};
use work_core::api::{
    HostResponse, PendingDialog, ProviderCatalog, ProviderMutation, ProviderReceipt,
    SessionActivity, SessionSummary,
};
use work_core::{ApplyResult, MessageBlock, RunProjection, ToolStatus};
use work_viewmodel::observability::metrics::{Context, Metrics};

pub struct ProjectItem {
    pub id: String,
    pub name: String,
    pub path: String,
}

impl From<Project> for ProjectItem {
    fn from(project: Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
        }
    }
}

pub struct SessionItem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub updated_at: u64,
    pub archived: bool,
    pub pinned: bool,
    pub activity: Option<SessionActivity>,
}

impl SessionItem {
    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            "未命名会话"
        } else {
            &self.title
        }
    }
}

impl From<SessionSummary> for SessionItem {
    fn from(session: SessionSummary) -> Self {
        Self {
            id: session.session_id,
            project_id: session.project_id.unwrap_or_default(),
            title: session.title.unwrap_or_default(),
            updated_at: session.updated_at,
            archived: session.archived,
            pinned: session.pinned,
            activity: session.activity,
        }
    }
}

pub(crate) struct ProviderWrite {
    pub receipt: Option<ProviderReceipt>,
    pub observed: Option<ProviderCatalog>,
}

pub enum Creation {
    Created(SessionItem),
    WriteFailed(ClientError),
    ReadFailed { id: String, error: ClientError },
}

/// Initial project and session list. Sample conversations are supplied only in sample mode.
pub struct Snapshot {
    pub node: String,
    pub node_id: Option<String>,
    pub mode: String,
    pub projects: Vec<ProjectItem>,
    pub sessions: Vec<SessionItem>,
    /// Canonical history messages (the `Value` shape the client already uses).
    pub history: Vec<Value>,
    /// Append-only run projection blocks for the selected session.
    pub blocks: Vec<MessageBlock>,
    pub metrics: Metrics,
    pub context: Context,
}

impl Snapshot {
    fn connected(
        origin: String,
        node_id: Option<String>,
        projects: Vec<Project>,
        sessions: Vec<SessionSummary>,
    ) -> Self {
        Self {
            node: origin,
            node_id,
            mode: "已连接节点".into(),
            projects: projects.into_iter().map(ProjectItem::from).collect(),
            sessions: sessions.into_iter().map(SessionItem::from).collect(),
            history: Vec::new(),
            blocks: Vec::new(),
            metrics: Metrics::default(),
            context: Context::default(),
        }
    }

    pub(super) fn disconnected() -> Self {
        let mut snapshot = Self::connected(String::new(), None, Vec::new(), Vec::new());
        snapshot.mode = "未连接".into();
        snapshot
    }

    /// Same content as the browser fixture: two projects, four sessions, one
    /// seeded run with a read + bash tool pair.
    pub fn fixture() -> Self {
        let read_output = "pub fn theme() { /* test fixture */ }";
        Self {
            node: "fixture-node".into(),
            node_id: None,
            mode: "示例 · 未连接".into(),
            projects: vec![
                ProjectItem {
                    id: "rpi-work".into(),
                    name: "rpi-work".into(),
                    path: String::new(),
                },
                ProjectItem {
                    id: "pi-rust".into(),
                    name: "pi-rust".into(),
                    path: String::new(),
                },
            ],
            sessions: vec![
                SessionItem {
                    id: "dark".into(),
                    project_id: "rpi-work".into(),
                    title: "为工作台添加深色模式".into(),
                    updated_at: 1_792_910_000_000,
                    archived: false,
                    pinned: false,
                    activity: Some(SessionActivity::Inactive),
                },
                SessionItem {
                    id: "architecture".into(),
                    project_id: "rpi-work".into(),
                    title: "梳理 Agent 架构".into(),
                    updated_at: 1_792_900_000_000,
                    archived: false,
                    pinned: false,
                    activity: Some(SessionActivity::Inactive),
                },
                SessionItem {
                    id: "mobile".into(),
                    project_id: "rpi-work".into(),
                    title: "检查手机端布局".into(),
                    updated_at: 1_792_890_000_000,
                    archived: false,
                    pinned: false,
                    activity: Some(SessionActivity::Inactive),
                },
                SessionItem {
                    id: "rpc".into(),
                    project_id: "pi-rust".into(),
                    title: "验证 RPC 事件协议".into(),
                    updated_at: 1_792_880_000_000,
                    archived: false,
                    pinned: false,
                    activity: Some(SessionActivity::Inactive),
                },
            ],
            history: vec![json!({
                "id": "user-dark",
                "role": "user",
                "content": "帮我为工作台增加一个深色主题，并保持手机端可用。",
            })],
            blocks: vec![
                MessageBlock::Text {
                    id: "answer".into(),
                    text: "主题切换已经准备好，接下来运行组件测试。".into(),
                },
                MessageBlock::ToolCall {
                    id: "read-1".into(),
                    name: "read".into(),
                    arguments: json!({ "path": "src/theme.rs" }).to_string(),
                    output: read_output.into(),
                    status: ToolStatus::Completed,
                },
                MessageBlock::ToolCall {
                    id: "shell-1".into(),
                    name: "bash".into(),
                    arguments:
                        json!({ "command": "cargo test --manifest-path apps/work-gpui/Cargo.toml" })
                            .to_string(),
                    output: "12 passed; 0 failed (test fixture)".into(),
                    status: ToolStatus::Completed,
                },
            ],
            metrics: Metrics::session(Some(&json!({
                "uncachedTokens": 8000,
                "cacheReadTokens": 24000,
                "cacheWriteTokens": 4800,
                "outputTokens": 4800,
                "costTotal": 0.0184,
                "requests": 3,
            }))),
            context: Context::reported(Some(&json!({
                "tokens": 38000,
                "window": 100000,
                "percent": 38,
                "threshold": 84000,
            }))),
        }
    }
}

pub(super) struct ResourceManagement {
    pub receipt: Result<work_core::api::NativeReceipt, ClientError>,
    pub observed: Option<work_core::api::NativeCatalog>,
}

pub(super) struct ProjectResourceWrite {
    pub receipt: Result<work_core::api::NativeOperation, ClientError>,
    pub observed: Option<work_core::api::NativeProjectResources>,
}

pub(crate) struct NodeAuthorization {
    pub(crate) ticket: String,
    pub(crate) node_id: String,
    pub(crate) expires_at: u64,
}

pub struct Node {
    client: WorkspaceClient,
    runtime: tokio::runtime::Runtime,
}

impl Node {
    pub fn connect(origin: &str, password: Option<&str>) -> Result<(Snapshot, Self), String> {
        let runtime = Self::runtime()?;
        let client =
            WorkspaceClient::new(origin).map_err(|error| format!("节点地址无效：{error}"))?;
        let snapshot = runtime.block_on(async {
            let auth = client
                .auth()
                .await
                .map_err(|error| format!("读取认证状态失败：{error}"))?;
            if auth.required && !auth.authenticated {
                let password = password.ok_or("节点需要密码：设置 RPI_WORK_PASSWORD 后重试")?;
                client
                    .login(password)
                    .await
                    .map_err(|error| format!("登录失败：{error}"))?;
            }
            let projects = client
                .projects()
                .await
                .map_err(|error| format!("项目读取失败：{error}"))?;
            let sessions = client
                .sessions()
                .await
                .map_err(|error| format!("会话读取失败：{error}"))?;
            let node_id =
                work_viewmodel::config_requests::bounded("读取节点身份", client.node_identity())
                    .await
                    .ok();
            Ok::<_, String>(Snapshot::connected(
                origin.to_owned(),
                node_id,
                projects,
                sessions,
            ))
        })?;
        Ok((snapshot, Self { client, runtime }))
    }

    pub(super) fn local() -> Result<(Snapshot, Self), String> {
        let client = work_host::client().ok_or("本机工作区尚未就绪。")?;
        let runtime = Self::runtime()?;
        let mut snapshot = runtime
            .block_on(async {
                use work_viewmodel::config_requests::bounded;

                let (projects, sessions) = tokio::join!(
                    bounded("读取本机项目", client.projects()),
                    bounded("读取本机会话", client.sessions()),
                );
                Ok::<_, ClientError>(Snapshot::connected(
                    "rpi-work://local".into(),
                    None,
                    projects?,
                    sessions?,
                ))
            })
            .map_err(|error| error.to_string())?;
        snapshot.mode = "此电脑".into();
        Ok((snapshot, Self { client, runtime }))
    }

    fn runtime() -> Result<tokio::runtime::Runtime, String> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| format!("无法创建运行时：{error}"))
    }

    pub(super) fn open(
        origin: String,
        expected: Option<String>,
        credential: Option<String>,
        authorization: Option<NodeAuthorization>,
    ) -> Result<Option<(Snapshot, Self)>, String> {
        use work_viewmodel::connection::{ConnectionOutcome, open, with_timeout};

        let runtime = Self::runtime()?;
        let client = WorkspaceClient::new(&origin).map_err(|_| "节点地址无效。")?;
        let using_authorization = authorization.is_some();
        let expected = authorization
            .as_ref()
            .map(|code| code.node_id.clone())
            .or(expected);
        let opened = runtime.block_on(async {
            if let Some(code) = authorization {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_secs());
                if code.expires_at <= now {
                    return Err("授权已过期，请重新扫码。".into());
                }
                let authorized = with_timeout(async {
                    if client.qr_authorization_node().await?.as_deref() != Some(&code.node_id) {
                        return Ok(false);
                    }
                    client
                        .redeem_qr_authorization(&code.ticket)
                        .await
                        .map(|auth| auth.authenticated)
                })
                .await;
                if !matches!(authorized, Ok(Ok(true))) {
                    return Err("授权失败，请生成新的二维码后重试。".into());
                }
            } else if let Some(expected) = expected.as_deref() {
                let observed = with_timeout(client.node_identity()).await?.ok();
                if observed.as_deref() != Some(expected) {
                    return Err("发现的节点身份已变化，请刷新附近节点。".into());
                }
            }
            open(client.clone(), credential).await
        })?;
        match opened {
            ConnectionOutcome::PasswordRequired(_) if using_authorization => {
                Err("节点未确认授权，请生成新的二维码后重试。".into())
            }
            ConnectionOutcome::PasswordRequired(_) => Ok(None),
            ConnectionOutcome::Ready(workspace) => {
                if expected.is_some() && expected != workspace.node_id {
                    return Err("发现的节点身份已变化，请刷新附近节点。".into());
                }
                let snapshot = Snapshot::connected(
                    origin,
                    workspace.node_id,
                    workspace.projects,
                    workspace.sessions,
                );
                Ok(Some((snapshot, Self { client, runtime })))
            }
        }
    }

    pub(super) fn local_sharing(
        &self,
        change: Option<(&'static str, bool)>,
    ) -> JoinHandle<(work_host::http::Sharing, Option<String>)> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let error = if let Some((capability, enabled)) = change {
                work_viewmodel::config_requests::bounded(
                    "更新本机协同权限",
                    client
                        .component_capability(capability, serde_json::json!({"enabled": enabled})),
                )
                .await
                .err()
                .map(|error| error.to_string())
            } else {
                None
            };
            (work_host::http::sharing().await, error)
        })
    }

    pub(super) fn discover_nearby(
        &self,
    ) -> JoinHandle<Result<Vec<work_client::DiscoveredNode>, ClientError>> {
        self.runtime.spawn(work_viewmodel::config_requests::bounded(
            "发现附近节点",
            work_client::discover_nodes(),
        ))
    }

    pub(super) fn prepare_node_share(
        &self,
        address: String,
        expected: String,
        authorization: bool,
    ) -> JoinHandle<Result<Option<work_client::QrAuthorization>, String>> {
        let local = self.is_local();
        self.runtime.spawn(async move {
            if authorization {
                if !local {
                    return Err("请在所属电脑签发授权。".into());
                }
                let observed = work_host::http::authorization_endpoint_node(&address).await?;
                if observed != expected {
                    return Err("分享地址与当前节点不匹配。".into());
                }
                return work_host::http::issue_qr_authorization().await.map(Some);
            }
            let client = WorkspaceClient::new(&address).map_err(|error| error.to_string())?;
            if matches!(
                work_viewmodel::connection::with_timeout(client.node_identity()).await,
                Ok(Ok(identity)) if identity == expected,
            ) {
                Ok(None)
            } else {
                Err("无法在此地址验证当前节点。请检查地址与节点监听设置。".into())
            }
        })
    }

    pub(super) fn read_provider_catalog(
        &self,
    ) -> JoinHandle<Result<work_core::api::ProviderCatalog, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取模型供应商", client.provider_catalog())
                .await
        })
    }

    pub(super) fn mutate_provider(&self, request: ProviderMutation) -> JoinHandle<ProviderWrite> {
        use work_viewmodel::config_requests::bounded;

        let client = self.client.clone();
        self.runtime.spawn(async move {
            let receipt = bounded("保存模型供应商配置", client.provider_mutate(request))
                .await
                .ok();
            let observed = bounded("回读模型供应商配置", client.provider_catalog())
                .await
                .ok();
            ProviderWrite { receipt, observed }
        })
    }

    pub(super) fn read_native_catalog(
        &self,
        project: String,
    ) -> JoinHandle<Result<work_core::api::NativeCatalog, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded(
                "读取资源目录",
                client.native_catalog(&project),
            )
            .await
        })
    }

    pub(super) fn manage_native_resource(
        &self,
        project: String,
        request: work_core::api::NativeManagement,
    ) -> JoinHandle<ResourceManagement> {
        use work_viewmodel::config_requests::bounded;
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let receipt = bounded("提交资源操作", client.native_manage(&project, request)).await;
            let observed = bounded("回读资源目录", client.native_catalog(&project))
                .await
                .ok();
            ResourceManagement { receipt, observed }
        })
    }

    pub(super) fn read_project_resources(
        &self,
        project: String,
    ) -> JoinHandle<Option<work_core::api::NativeProjectResources>> {
        use work_viewmodel::config_requests::bounded;
        let client = self.client.clone();
        self.runtime.spawn(async move {
            bounded(
                "读取项目资源配置",
                client.native_project_resources(&project),
            )
            .await
            .ok()
        })
    }

    pub(super) fn write_project_resources(
        &self,
        project: String,
        request: work_core::api::NativeApply,
    ) -> JoinHandle<ProjectResourceWrite> {
        use work_viewmodel::config_requests::bounded;
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let receipt = bounded(
                "保存项目资源配置",
                client.native_apply_project(&project, request),
            )
            .await;
            let observed = bounded(
                "回读项目资源配置",
                client.native_project_resources(&project),
            )
            .await
            .ok();
            ProjectResourceWrite { receipt, observed }
        })
    }

    pub(super) fn inspect_native_resource(
        &self,
        project: String,
        reference: String,
    ) -> JoinHandle<Result<work_core::api::NativeReceipt, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let request = work_core::api::NativeManagement::Inspect { reference };
            work_viewmodel::config_requests::bounded(
                "检查已安装资源",
                client.native_manage(&project, request),
            )
            .await
        })
    }

    pub fn is_local(&self) -> bool {
        self.client.is_local()
    }

    pub(super) fn sessions(&self) -> JoinHandle<Result<Vec<SessionItem>, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取会话状态", client.sessions())
                .await
                .map(|sessions| sessions.into_iter().map(SessionItem::from).collect())
        })
    }

    pub(super) fn fork_session(
        &self,
        request: crate::shell::session_fork::Request,
        readback: Option<String>,
    ) -> JoinHandle<(
        crate::shell::session_fork::Request,
        crate::shell::session_fork::Outcome,
    )> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let outcome = request.run(&client, readback).await;
            (request, outcome)
        })
    }

    pub fn watch(
        &self,
        id: String,
    ) -> (
        mpsc::Receiver<Result<SessionFrame, ClientError>>,
        AbortHandle,
    ) {
        let client = self.client.clone();
        let (output, rx) = mpsc::channel(4);
        let task = self.runtime.spawn(async move {
            if let Err(error) = watch_session(&client, &id, &output).await {
                let _ = output.send(Err(error)).await;
            }
        });
        (rx, task.abort_handle())
    }

    pub fn create_session(
        &self,
        project_id: String,
        known_id: Option<String>,
        verify_listing: bool,
    ) -> JoinHandle<Creation> {
        use work_viewmodel::config_requests::bounded;

        let client = self.client.clone();
        self.runtime.spawn(async move {
            let id = match known_id {
                Some(id) => id,
                None => match bounded("新建对话", client.create_session(Some(&project_id))).await
                {
                    Ok(id) if !id.trim().is_empty() => id,
                    Ok(_) => {
                        return Creation::WriteFailed(ClientError::Protocol(
                            "节点未返回会话标识".into(),
                        ));
                    }
                    Err(error) => return Creation::WriteFailed(error),
                },
            };
            let summary = bounded("核对新会话", client.get_session(&id))
                .await
                .and_then(|summary| {
                    if summary.session_id == id
                        && summary.project_id.as_deref() == Some(project_id.as_str())
                    {
                        Ok(summary)
                    } else {
                        Err(ClientError::Protocol("新会话身份与请求不一致".into()))
                    }
                });
            let summary = match summary {
                Ok(summary) if verify_listing => bounded("核对独立审阅会话列表", client.sessions())
                    .await
                    .and_then(|listed| {
                        listed
                            .into_iter()
                            .find(|item| {
                                item.session_id == summary.session_id
                                    && !item.archived
                                    && item.project_id == summary.project_id
                            })
                            .ok_or_else(|| {
                                ClientError::Protocol("会话列表尚未确认新的独立审阅会话".into())
                            })
                    }),
                other => other,
            };
            match summary {
                Ok(summary) => Creation::Created(summary.into()),
                Err(error) => Creation::ReadFailed { id, error },
            }
        })
    }

    pub(crate) fn read_capabilities(
        &self,
    ) -> JoinHandle<Result<work_core::api::Capabilities, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取节点操作能力", client.capabilities())
                .await
        })
    }

    pub(crate) fn edit_session(
        &self,
        request: crate::shell::session_management::Request,
        verify_only: bool,
    ) -> JoinHandle<crate::shell::session_management::Outcome> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { request.run(&client, verify_only).await })
    }

    pub fn observe_config(
        &self,
        id: String,
    ) -> JoinHandle<Result<crate::shell::composer_config::Observed, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { crate::shell::composer_config::observe(&client, &id).await })
    }

    pub fn config_choices(
        &self,
        id: String,
        field: work_viewmodel::config_requests::ConfigField,
    ) -> JoinHandle<crate::shell::composer_config::CatalogResult> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { crate::shell::composer_config::choices(&client, &id, field).await })
    }

    pub fn update_config(
        &self,
        id: String,
        choice: crate::shell::composer_config::Choice,
        evidence: Option<crate::shell::composer_config::Evidence>,
    ) -> JoinHandle<crate::shell::composer_config::ResultState> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            crate::shell::composer_config::update(&client, &id, &choice, evidence).await
        })
    }

    pub fn project_folders(&self, path: String) -> JoinHandle<Result<ProjectFolders, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取节点目录", client.project_folders(&path))
                .await
        })
    }

    pub fn register_project(
        &self,
        request: crate::shell::project_management::Registration,
    ) -> JoinHandle<crate::shell::project_management::ResultState> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { request.run(&client).await })
    }

    pub fn read_project(
        &self,
        id: String,
        route: crate::shell::project_files::Route,
    ) -> JoinHandle<Result<crate::shell::project_files::Contents, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { route.read(&client, &id).await })
    }

    pub(crate) fn project_line_counts(
        &self,
        project: String,
        files: Vec<work_core::api::ChangedFile>,
    ) -> (
        mpsc::Receiver<(String, work_viewmodel::diff_view::LineCounts)>,
        AbortHandle,
    ) {
        let client = self.client.clone();
        let (output, receiver) = mpsc::channel(4);
        let task = self.runtime.spawn(async move {
            crate::shell::project_files::read_line_counts(&client, &project, files, output).await;
        });
        (receiver, task.abort_handle())
    }

    pub fn prompt(
        &self,
        id: String,
        text: String,
    ) -> JoinHandle<Result<HostResponse<Value>, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.prompt(&id, &text).await })
    }

    pub fn prompt_content(
        &self,
        id: String,
        content: Value,
    ) -> JoinHandle<Result<HostResponse<Value>, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.prompt_content(&id, &content).await })
    }

    pub(super) fn read_runtime_state(
        &self,
        id: String,
    ) -> JoinHandle<Result<work_client::RuntimeState, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取会话上下文", client.runtime_state(&id))
                .await
        })
    }

    pub fn read_queue(
        &self,
        id: String,
    ) -> JoinHandle<Result<work_client::QueueSnapshot, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded("读取消息队列", client.queue(&id, false)).await
        })
    }

    pub fn queue_command(
        &self,
        id: String,
        command: Value,
    ) -> JoinHandle<Result<HostResponse<Value>, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.command(&id, command).await })
    }

    pub fn control(
        &self,
        id: String,
        action: crate::shell::session_action::Action,
    ) -> JoinHandle<Result<crate::shell::session_action::ControlAdmission, ClientError>> {
        use crate::shell::session_action::{Action, abort_admission, dialog_admission};

        let client = self.client.clone();
        self.runtime.spawn(async move {
            match action {
                Action::Abort => client.abort(&id).await.map(abort_admission),
                Action::Reply { request_id, answer } => {
                    let reply = client
                        .dialog_response(&id, &request_id, answer.payload())
                        .await?;
                    Ok(dialog_admission(&reply).map(|result| result.map(|()| None)))
                }
            }
        })
    }

    pub fn receipt(
        &self,
        operation: OperationRef,
    ) -> JoinHandle<Result<OperationStatus, ClientError>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            work_viewmodel::config_requests::bounded(
                "读取操作回执",
                client.operation_status(&operation),
            )
            .await
        })
    }
}

pub struct SessionFrame {
    pub session_id: String,
    pub history: Vec<Value>,
    pub projection: RunProjection,
    pub access: Option<SessionAccess>,
    pub dialogs: Vec<PendingDialog>,
    pub pending_resume: bool,
    pub event: String,
}

impl SessionFrame {
    fn capture(
        connection: &SessionConnection,
        access: Option<&SessionAccess>,
        event: String,
    ) -> Self {
        Self {
            session_id: connection.session_id().into(),
            history: connection.messages().to_vec(),
            projection: connection.projection().clone(),
            access: access.cloned(),
            dialogs: connection.pending_dialogs().to_vec(),
            pending_resume: connection.pending_resume(),
            event,
        }
    }
}

async fn watch_session(
    client: &WorkspaceClient,
    id: &str,
    output: &mpsc::Sender<Result<SessionFrame, ClientError>>,
) -> Result<(), ClientError> {
    use work_viewmodel::config_requests::bounded;
    let initial_access = bounded("读取会话能力", client.session_access(id)).await?;
    let mut connection = client
        .connect_session_with_access(id, &initial_access)
        .await?;
    let mut access = Some(initial_access);
    if output
        .send(Ok(SessionFrame::capture(
            &connection,
            access.as_ref(),
            String::new(),
        )))
        .await
        .is_err()
    {
        return Ok(());
    }
    while let Some(event) = connection.next().await? {
        if matches!(
            event.applied,
            ApplyResult::IgnoredStale | ApplyResult::IgnoredDuplicate
        ) {
            continue;
        }
        let kind = event.wire.event_type;
        let refresh = matches!(kind.as_str(), "connection_online" | "agent_settled");
        if refresh || matches!(kind.as_str(), "connection_reconnecting" | "resync_required") {
            access = None;
        }
        if output
            .send(Ok(SessionFrame::capture(
                &connection,
                access.as_ref(),
                kind,
            )))
            .await
            .is_err()
        {
            return Ok(());
        }
        if refresh {
            access = bounded("读取会话能力", client.session_access(id))
                .await
                .ok();
            if output
                .send(Ok(SessionFrame::capture(
                    &connection,
                    access.as_ref(),
                    "access".into(),
                )))
                .await
                .is_err()
            {
                return Ok(());
            }
        }
    }
    Err(ClientError::Protocol("会话事件流已结束".into()))
}
