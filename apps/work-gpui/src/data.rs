//! Workbench data from an authenticated Work node or the layout sample.

use serde_json::{Value, json};
use work_client::WorkspaceClient;
use work_core::{MessageBlock, ToolStatus};
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
                mode: "已连接节点 · 只读".into(),
                projects: projects
                    .into_iter()
                    .map(|project| ProjectItem {
                        id: project.id,
                        name: project.name,
                    })
                    .collect(),
                sessions: sessions
                    .into_iter()
                    .map(|session| SessionItem {
                        id: session.session_id,
                        project_id: session.project_id.unwrap_or_default(),
                        title: session.title.unwrap_or_default(),
                        updated_at: session.updated_at,
                    })
                    .collect(),
                history: Vec::new(),
                blocks: Vec::new(),
                metrics: Metrics::default(),
                context: Context::default(),
            })
        })?;
        Ok((snapshot, Self { client, runtime }))
    }

    pub fn open(
        &self,
        session_id: String,
    ) -> tokio::task::JoinHandle<Result<work_client::SessionConnection, work_client::ClientError>>
    {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.connect_session(session_id).await })
    }
}
