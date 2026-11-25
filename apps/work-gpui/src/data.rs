//! Workbench data from an authenticated Work node or the layout sample.

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};
use work_client::{ClientError, SessionAccess, SessionConnection, WorkspaceClient};
use work_core::api::realtime::{OperationRef, OperationStatus};
use work_core::api::{HostResponse, PendingDialog, SessionSummary};
use work_core::{ApplyResult, MessageBlock, RunProjection, ToolStatus};
use work_viewmodel::observability::metrics::{Context, Metrics};

pub struct ProjectItem {
    pub id: String,
    pub name: String,
}

pub struct SessionItem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub updated_at: u64,
}

impl From<SessionSummary> for SessionItem {
    fn from(session: SessionSummary) -> Self {
        Self {
            id: session.session_id,
            project_id: session.project_id.unwrap_or_default(),
            title: session.title.unwrap_or_default(),
            updated_at: session.updated_at,
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
                },
                SessionItem {
                    id: "architecture".into(),
                    project_id: "rpi-work".into(),
                    title: "梳理 Agent 架构".into(),
                    updated_at: 1_792_900_000_000,
                },
                SessionItem {
                    id: "mobile".into(),
                    project_id: "rpi-work".into(),
                    title: "检查手机端布局".into(),
                    updated_at: 1_792_890_000_000,
                },
                SessionItem {
                    id: "rpc".into(),
                    project_id: "pi-rust".into(),
                    title: "验证 RPC 事件协议".into(),
                    updated_at: 1_792_880_000_000,
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
                projects: projects
                    .into_iter()
                    .map(|project| ProjectItem {
                        id: project.id,
                        name: project.name,
                    })
                    .collect(),
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
            match summary {
                Ok(summary) => Creation::Created(summary.into()),
                Err(error) => Creation::ReadFailed { id, error },
            }
        })
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

    pub fn prompt(
        &self,
        id: String,
        text: String,
    ) -> JoinHandle<Result<HostResponse<Value>, ClientError>> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.prompt(&id, &text).await })
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
                Action::Reply { request_id, value } => {
                    let reply = client
                        .dialog_response(
                            &id,
                            &request_id,
                            json!({"kind": "selector", "values": [value]}),
                        )
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
