//! Workbench data from an authenticated Work node or the layout sample.

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};
use work_client::{
    ClientError, Project, ProjectFolders, SessionAccess, SessionConnection, WorkspaceClient,
};
use work_core::api::realtime::{OperationRef, OperationStatus};
use work_core::api::{HostResponse, PendingDialog, SessionSummary};
use work_core::{ApplyResult, MessageBlock, RunProjection, ToolStatus};
use work_viewmodel::observability::metrics::{Context, Metrics};

pub struct ProjectItem {
    pub id: String,
    pub name: String,
}

impl From<Project> for ProjectItem {
    fn from(project: Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
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
        }
    }
}

pub enum Creation {
    Created(SessionItem),
    WriteFailed(ClientError),
    ReadFailed { id: String, error: ClientError },
}

/// Initial project and session list. Sample conversations are supplied only in sample mode.
pub struct Snapshot {
    pub node: String,
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
    /// Same content as the browser fixture: two projects, four sessions, one
    /// seeded run with a read + bash tool pair.
    pub fn fixture() -> Self {
        let read_output = "pub fn theme() { /* test fixture */ }";
        Self {
            node: "fixture-node".into(),
            mode: "示例 · 未连接".into(),
            projects: vec![
                ProjectItem {
                    id: "rpi-work".into(),
                    name: "rpi-work".into(),
                },
                ProjectItem {
                    id: "pi-rust".into(),
                    name: "pi-rust".into(),
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
                },
                SessionItem {
                    id: "architecture".into(),
                    project_id: "rpi-work".into(),
                    title: "梳理 Agent 架构".into(),
                    updated_at: 1_792_900_000_000,
                    archived: false,
                    pinned: false,
                },
                SessionItem {
                    id: "mobile".into(),
                    project_id: "rpi-work".into(),
                    title: "检查手机端布局".into(),
                    updated_at: 1_792_890_000_000,
                    archived: false,
                    pinned: false,
                },
                SessionItem {
                    id: "rpc".into(),
                    project_id: "pi-rust".into(),
                    title: "验证 RPC 事件协议".into(),
                    updated_at: 1_792_880_000_000,
                    archived: false,
                    pinned: false,
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

pub struct Node {
    client: WorkspaceClient,
    runtime: tokio::runtime::Runtime,
}

impl Node {
    pub fn connect(origin: &str, password: Option<&str>) -> Result<(Snapshot, Self), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| format!("无法创建运行时：{error}"))?;
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
            Ok::<_, String>(Snapshot {
                node: origin.to_owned(),
                mode: "已连接节点".into(),
                projects: projects.into_iter().map(ProjectItem::from).collect(),
                sessions: sessions.into_iter().map(SessionItem::from).collect(),
                history: Vec::new(),
                blocks: Vec::new(),
                metrics: Metrics::default(),
                context: Context::default(),
            })
        })?;
        Ok((snapshot, Self { client, runtime }))
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

    pub fn control(
        &self,
        id: String,
        action: crate::shell::session_action::Action,
    ) -> JoinHandle<Result<Option<Result<(), String>>, ClientError>> {
        use crate::shell::session_action::Action;
        use work_viewmodel::session_receipts::command_admission;

        let client = self.client.clone();
        self.runtime.spawn(async move {
            match action {
                Action::Abort => client
                    .abort(&id)
                    .await
                    .map(|reply| command_admission(reply, Some("abort"), false)),
                Action::Reply { request_id, answer } => {
                    let reply = client
                        .dialog_response(&id, &request_id, answer.payload())
                        .await?;
                    Ok(crate::shell::session_action::dialog_admission(&reply))
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
