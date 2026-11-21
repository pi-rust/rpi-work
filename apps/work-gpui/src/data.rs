//! Data sources for the prototype.
//!
//! `fixture()` mirrors `tests/fixtures/ui-fixture.cjs` so the GPUI window and
//! the Dioxus Web client can be compared against the same content. `connect()`
//! is the read-only slice of `work-client` reuse (v1 of the prototype plan).

use serde_json::{Value, json};
use work_core::{MessageBlock, ToolStatus};
use work_viewmodel::observability::metrics::{Context, Metrics};

#[derive(Clone)]
pub struct ProjectItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct SessionItem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub updated_at: u64,
}

/// One immutable workspace snapshot; the prototype never mutates the node.
#[derive(Clone)]
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
    pub tools_loaded: usize,
    pub tools_failed: usize,
}

impl Snapshot {
    /// Same content as the browser fixture: two projects, four sessions, one
    /// seeded run with a read + bash tool pair.
    pub fn fixture() -> Self {
        let read_output = "pub fn theme() { /* test fixture */ }";
        Self {
            node: "fixture-node".into(),
            mode: "本地节点 · fixture".into(),
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
                    arguments: json!({ "command": "cargo test -p work-ui --lib" }).to_string(),
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
            tools_loaded: 2,
            tools_failed: 0,
        }
    }

    /// Read-only snapshot from a running node through `work-client`.
    pub fn connect(origin: &str, password: Option<&str>) -> Result<Self, String> {
        use work_client::WorkspaceClient;

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("无法创建运行时：{error}"))?;
        let client =
            WorkspaceClient::new(origin).map_err(|error| format!("节点地址无效：{error}"))?;
        let password_owned = password.map(str::to_owned);
        runtime.block_on(async move {
            let auth = client
                .auth()
                .await
                .map_err(|error| format!("读取认证状态失败：{error}"))?;
            if auth.required && !auth.authenticated {
                let Some(password) = password_owned else {
                    return Err("节点需要密码：设置 RPI_WORK_PASSWORD 后重试".into());
                };
                client
                    .login(&password)
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
            let metrics = Metrics::session(None);
            Ok(Self {
                node: origin.to_string(),
                mode: format!("远程节点 · {origin}"),
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
                metrics,
                context: Context::default(),
                tools_loaded: 0,
                tools_failed: 0,
            })
        })
    }
}
