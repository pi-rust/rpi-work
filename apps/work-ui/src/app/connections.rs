use dioxus::prelude::*;
use serde_json::{Value, json};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use work_client::{AuthState, Capabilities, Project, SessionSummary, WorkspaceClient};
use work_core::AgentDescriptor;

#[derive(Clone, Default, PartialEq)]
pub struct NodeHistory {
    pub recent: Vec<String>,
    pub selected: Option<String>,
    /// Canonical address to stable node identity, used only to collapse aliases.
    node_ids: std::collections::BTreeMap<String, String>,
}

impl NodeHistory {
    fn restore(value: &Value) -> Self {
        if value["version"].as_u64() != Some(1) {
            return Self::default();
        }
        let mut history = Self {
            selected: value["selected"].as_str().and_then(|raw| {
                selection_address(
                    raw,
                    cfg!(not(any(target_arch = "wasm32", target_os = "android"))),
                )
            }),
            ..Self::default()
        };
        for raw in value["recent"].as_array().into_iter().flatten() {
            if let Some(address) = raw
                .as_str()
                .and_then(|raw| normalize_address(raw, false).ok())
            {
                if !history.recent.contains(&address) {
                    history.recent.push(address);
                }
            }
            if history.recent.len() == 8 {
                break;
            }
        }
        if let Some(entries) = value["nodeIds"].as_object() {
            for (raw_address, raw_id) in entries {
                let Some(address) = normalize_address(raw_address, false).ok() else {
                    continue;
                };
                let Some(node_id) = raw_id.as_str().filter(|id| valid_node_id(id)) else {
                    continue;
                };
                if history.recent.iter().any(|item| item == &address) {
                    history.node_ids.insert(address, node_id.to_owned());
                }
            }
        }
        history
    }

    /// Record a successfully connected node, never a draft or failed address.
    /// The local workspace is a selection, not a remote recent entry.
    pub fn select(&mut self, address: &str) {
        self.select_with_identity(address, None);
    }

    /// Record a successful connection and collapse known aliases of the same node.
    pub fn select_with_identity(&mut self, address: &str, node_id: Option<&str>) {
        let Some(address) = selection_address(address, true) else {
            return;
        };
        if address != "rpi-work://local" {
            if let Some(node_id) = node_id.filter(|id| valid_node_id(id)) {
                let aliases: Vec<_> = self
                    .recent
                    .iter()
                    .filter(|item| {
                        *item != &address
                            && self
                                .node_ids
                                .get(*item)
                                .is_some_and(|known| known == node_id)
                    })
                    .cloned()
                    .collect();
                for alias in aliases {
                    self.recent.retain(|item| item != &alias);
                    self.node_ids.remove(&alias);
                }
                self.node_ids.insert(address.clone(), node_id.to_owned());
            }
            self.recent.retain(|item| item != &address);
            self.recent.insert(0, address.clone());
            self.recent.truncate(8);
            self.node_ids
                .retain(|known_address, _| self.recent.iter().any(|item| item == known_address));
        }
        self.selected = Some(address);
    }

    pub fn remember(&mut self, address: &str) {
        self.select(address);
    }

    pub fn remember_with_identity(&mut self, address: &str, node_id: Option<&str>) {
        if node_id.is_none() {
            self.remember(address);
        } else {
            self.select_with_identity(address, node_id);
        }
    }

    fn value(&self) -> Value {
        json!({"version":1,"recent":self.recent,"selected":self.selected,"nodeIds":self.node_ids})
    }
}

fn valid_node_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}

fn selection_address(raw: &str, allow_local: bool) -> Option<String> {
    if raw.trim() == "rpi-work://local" {
        return allow_local.then(|| "rpi-work://local".to_owned());
    }
    normalize_address(raw, false).ok()
}

/// Choose a startup address without I/O. Explicit configuration is returned
/// verbatim, even when invalid, so the connection UI can report its error.
/// Pass `allow_local = false` on Web/Android and a platform-appropriate fallback
/// (for example the Web origin or `about:blank`).
pub(crate) fn startup_address(
    explicit: Option<&str>,
    history: &NodeHistory,
    fallback: &str,
    allow_local: bool,
) -> String {
    if let Some(address) = explicit {
        return address.to_owned();
    }
    history
        .selected
        .as_deref()
        .and_then(|raw| selection_address(raw, allow_local))
        .or_else(|| {
            history
                .recent
                .iter()
                .find_map(|raw| normalize_address(raw, false).ok())
        })
        .unwrap_or_else(|| {
            if !allow_local && fallback.trim() == "rpi-work://local" {
                "about:blank".to_owned()
            } else {
                fallback.to_owned()
            }
        })
}

/// Pick an existing, visible session from this node only. Catalog order breaks
/// timestamp ties; pinned/archived entries must not override recency.
pub(crate) fn default_session_id(
    sessions: &[SessionSummary],
    projects: &[Project],
    hidden_project: impl Fn(&str) -> bool,
) -> Option<String> {
    sessions
        .iter()
        .filter(|session| {
            !session.archived
                && !session.session_id.trim().is_empty()
                && session.project_id.as_deref().is_none_or(|id| {
                    projects.iter().any(|project| project.id == id) && !hidden_project(id)
                })
        })
        .min_by_key(|session| std::cmp::Reverse(session.updated_at))
        .map(|session| session.session_id.clone())
}

/// An in-memory password draft, bound to the node it was entered for.
/// Never serialize this value or reuse it for another node.
#[derive(Clone, Default, PartialEq)]
pub struct NodePassword {
    pub value: String,
    address: Option<String>,
}

impl NodePassword {
    pub fn new(value: String, address: &str) -> Self {
        Self {
            value,
            address: normalize_address(address, false).ok(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Submission consumes the draft even if its scope does not match.
    pub fn take_for(&mut self, address: &str) -> Option<String> {
        let draft = std::mem::take(self);
        if !draft.value.is_empty()
            && draft.address.is_some()
            && draft.address == normalize_address(address, false).ok()
        {
            Some(draft.value)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod password_tests {
    use super::NodePassword;

    #[test]
    fn password_is_consumed_once_for_its_normalized_node_only() {
        let mut password = NodePassword::new("secret".into(), "http://example.com:3031/");
        assert_eq!(
            password.take_for("http://example.com:3031"),
            Some("secret".into())
        );
        assert!(password.is_empty());
        assert_eq!(password.take_for("http://example.com:3031"), None);
    }

    #[test]
    fn another_host_port_or_scheme_cannot_receive_the_password() {
        for target in [
            "http://other.example:3031",
            "http://example.com:3032",
            "https://example.com:3031",
        ] {
            let mut password = NodePassword::new("secret".into(), "http://example.com:3031");
            assert_eq!(password.take_for(target), None);
            assert!(password.is_empty());
        }
    }

    #[test]
    fn invalid_or_empty_drafts_never_produce_a_password() {
        for address in ["about:blank", "rpi-work://local", "not a node"] {
            let mut password = NodePassword::new("secret".into(), address);
            assert_eq!(password.take_for(address), None);
        }
        let mut password = NodePassword::new(String::new(), "https://example.com");
        assert_eq!(password.take_for("https://example.com"), None);
    }
}

#[derive(Clone, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    Connecting,
    Authenticating,
    PasswordRequired,
    Online,
    Failed(String),
}

impl Phase {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Connecting | Self::Authenticating)
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Idle => "未连接",
            Self::Connecting => "正在连接…",
            Self::Authenticating => "正在登录并加载项目…",
            Self::PasswordRequired => "需要节点密码",
            Self::Online => "在线 · 可选择会话或项目",
            Self::Failed(message) => message,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ConnectionStore {
    pub history: Signal<NodeHistory>,
    pub ready: Signal<bool>,
    pub warning: Signal<String>,
    pub phase: Signal<Phase>,
    pub address: Signal<String>,
    pub epoch: Signal<u64>,
}

pub fn use_connections(initial_address: &str) -> ConnectionStore {
    let mut store = ConnectionStore {
        history: use_signal(NodeHistory::default),
        ready: use_signal(|| false),
        warning: use_signal(String::new),
        phase: use_signal(Phase::default),
        address: use_signal(|| initial_address.to_owned()),
        epoch: use_signal(|| 0),
    };
    use_context_provider(|| store);
    use_effect(move || {
        spawn(async move {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            let value = native_path()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            #[cfg(any(target_arch = "wasm32", target_os = "android"))]
            let value = document::eval("try{dioxus.send(JSON.parse(localStorage.getItem('rpi-work-nodes-v1')||'null'));}catch(_){dioxus.send(null);}")
                .recv::<Value>().await.ok();
            store
                .history
                .set(value.as_ref().map(NodeHistory::restore).unwrap_or_default());
            store.ready.set(true);
        });
    });
    use_effect(move || {
        let ready = *store.ready.read();
        let history = store.history.read().clone();
        if !ready {
            return;
        }
        spawn(async move {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            let saved = persist_native(&history);
            #[cfg(any(target_arch = "wasm32", target_os = "android"))]
            let saved = {
                let literal = serde_json::to_string(&history.value().to_string()).unwrap();
                matches!(document::eval(&format!("try{{localStorage.setItem('rpi-work-nodes-v1',{literal});dioxus.send(true);}}catch(_){{dioxus.send(false);}}" )).recv::<bool>().await, Ok(true))
            };
            store.warning.set(
                if saved {
                    ""
                } else {
                    "无法保存连接地址；本次连接仍可使用。"
                }
                .into(),
            );
        });
    });
    store
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn native_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|path| std::path::PathBuf::from(path).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|path| std::path::PathBuf::from(path).join(".config"))
        });
    root.map(|path| path.join("rpi-work").join("nodes.json"))
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn persist_native(history: &NodeHistory) -> bool {
    let Some(path) = native_path() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, history.value().to_string()).is_ok()
        && std::fs::rename(temporary, path).is_ok()
}

pub fn command_supported(capabilities: &Capabilities, command: &str) -> bool {
    // An empty list is the legacy response shape: allow the request so the
    // endpoint can return its real error, but never infer support from a
    // version string alone when the node supplied an explicit list.
    capabilities.supported_commands.is_empty()
        || capabilities
            .supported_commands
            .iter()
            .any(|item| item == command)
}

pub fn capability_error(capabilities: &Capabilities, capability: &str) -> Option<String> {
    let (supported, source) = match capability {
        "fork" | "clone" | "session_metadata" => (
            capabilities.supported_actions.is_empty()
                || capabilities
                    .supported_actions
                    .iter()
                    .any(|item| item == capability),
            "操作",
        ),
        command => (command_supported(capabilities, command), "命令"),
    };
    if supported {
        None
    } else {
        let description = if source == "命令" {
            "不支持"
        } else {
            "未确认支持"
        };
        Some(format!(
            "当前节点协议 {} {description}{source}“{capability}”；已保持连接，提供降级功能。",
            capabilities.protocol
        ))
    }
}

pub struct Workspace {
    pub transport: AgentDescriptor,
    pub auth: AuthState,
    pub capabilities: Capabilities,
    pub sessions: Vec<SessionSummary>,
    pub projects: Vec<Project>,
    pub node_id: Option<String>,
}

pub enum ConnectionOutcome {
    PasswordRequired(AuthState),
    Ready(Box<Workspace>),
}

pub async fn open(
    client: WorkspaceClient,
    password: Option<String>,
) -> Result<ConnectionOutcome, String> {
    with_timeout(async move {
        let descriptor = client.descriptor();
        if !descriptor.supports("sessions") || !descriptor.supports("prompt") {
            return Err(format!(
                "当前 transport {} 不支持会话或 prompt。",
                descriptor.identity.protocol
            ));
        }
        let auth = if client.is_local() {
            AuthState {
                required: false,
                authenticated: true,
            }
        } else if let Some(password) = password {
            client
                .login(&password)
                .await
                .map_err(|error| format!("登录失败：{error}"))?
        } else {
            let health = client.health().await.map_err(|_| {
                "连接失败：请检查电脑节点是否启动、节点地址、网络与允许来源。".to_owned()
            })?;
            if !health.ok {
                return Err("节点未就绪，请稍后重试。".into());
            }
            client
                .auth()
                .await
                .map_err(|error| format!("认证状态读取失败：{error}"))?
        };
        if auth.required && !auth.authenticated {
            return Ok(ConnectionOutcome::PasswordRequired(auth));
        }
        // All three resources must be available before reporting a usable node.
        let capabilities = client
            .capabilities()
            .await
            .map_err(|error| format!("协议读取失败：{error}"))?;
        if capabilities.protocol.trim().is_empty() {
            return Err("节点能力响应缺少协议版本，未启用会话功能。".into());
        }
        let sessions = client
            .sessions()
            .await
            .map_err(|error| format!("会话加载失败：{error}"))?;
        let projects = client
            .projects()
            .await
            .map_err(|error| format!("项目加载失败：{error}"))?;
        // Identity is a best-effort alias key. Older nodes may not expose it;
        // that must not make an otherwise usable connection fail.
        let node_id = if client.is_local() {
            None
        } else {
            client.node_identity().await.ok()
        };
        Ok(ConnectionOutcome::Ready(Box::new(Workspace {
            transport: descriptor,
            auth,
            capabilities,
            sessions,
            projects,
            node_id,
        })))
    })
    .await?
}

pub(super) async fn with_timeout<T>(future: impl Future<Output = T>) -> Result<T, String> {
    let mut future = pin!(future);
    let mut timer = pin!(async {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(15_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    });
    poll_fn(move |context| {
        if let Poll::Ready(value) = future.as_mut().poll(context) {
            return Poll::Ready(Ok(value));
        }
        if timer.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err("连接超时，请检查电脑节点与网络后重试。".into()));
        }
        Poll::Pending
    })
    .await
}

/// Only an already authenticated entry may exempt its own browser origin from
/// the phone's remote-loopback restriction. Other nodes and QR addresses still
/// use the strict policy, regardless of responsive viewport size.
pub fn normalize_connection_address(
    raw: &str,
    phone: bool,
    authenticated_entry: Option<&str>,
) -> Result<String, String> {
    let normalized = normalize_address(raw, false)?;
    if authenticated_entry
        .and_then(|entry| normalize_address(entry, false).ok())
        .as_deref()
        == Some(normalized.as_str())
    {
        Ok(normalized)
    } else {
        normalize_address(raw, phone)
    }
}

pub fn normalize_address(raw: &str, phone: bool) -> Result<String, String> {
    let value = raw.trim();
    if value.len() > 2048 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("服务地址过长或包含无效字符。".into());
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err("请输入完整的 HTTP(S) 服务地址，例如 https://电脑地址:13030。".into());
    };
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err("服务地址只支持 HTTP 或 HTTPS。".into());
    }
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', '\\', '%']) {
        return Err("服务地址只包含协议、主机和端口，不包含路径、查询参数或账号密码。".into());
    }
    let (host, port) = if let Some(ipv6) = rest.strip_prefix('[') {
        let (host, suffix) = ipv6.split_once(']').ok_or("IPv6 地址需要完整方括号。")?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| "IPv6 地址无效。")?;
        let port = if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':').ok_or("端口格式无效。")?)
        };
        (host, port)
    } else {
        let (host, port) = rest
            .split_once(':')
            .map_or((rest, None), |(h, p)| (h, Some(p)));
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
            || host
                .trim_end_matches('.')
                .split('.')
                .any(|part| part.is_empty() || part.starts_with('-') || part.ends_with('-'))
        {
            return Err("服务地址的主机名无效。".into());
        }
        (host, port)
    };
    if let Some(port) = port {
        if port.is_empty()
            || !port.bytes().all(|c| c.is_ascii_digit())
            || port.parse::<u16>().ok().is_none_or(|p| p == 0)
        {
            return Err("端口必须是 1 到 65535 之间的数字。".into());
        }
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let last_label = host.rsplit('.').next().unwrap_or_default();
    let numeric_host = !host.contains(':')
        && (last_label.bytes().all(|byte| byte.is_ascii_digit()) || last_label.starts_with("0x"));
    if numeric_host && host.parse::<std::net::Ipv4Addr>().is_err() {
        return Err("IPv4 地址请使用四段十进制格式。".into());
    }
    let loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host == "0.0.0.0"
        || host.starts_with("127.")
        || host.parse::<std::net::Ipv6Addr>().is_ok_and(|ip| {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|v4| v4.is_loopback() || v4.is_unspecified())
        })
        || (host.bytes().all(|c| c.is_ascii_digit())
            && host
                .parse::<u32>()
                .is_ok_and(|ip| std::net::Ipv4Addr::from(ip).is_loopback() || ip == 0));
    if phone && loopback {
        return Err(
            "手机上的 localhost / 127.0.0.1 指向手机本机，请填写电脑可达的局域网或远程地址。"
                .into(),
        );
    }
    Ok(format!("{scheme}://{}", rest.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_session_selects_latest_visible_without_creating_or_preferring_pins() {
        let session = |id: &str, updated: u64, project: Option<&str>, archived: bool| {
            serde_json::from_value::<SessionSummary>(json!({
                "sessionId": id, "projectId": project, "status": "idle", "title": id,
                "createdAt": 0, "updatedAt": updated, "lastSeq": 0, "archived": archived
            }))
            .unwrap()
        };
        let projects = vec![Project {
            id: "project".into(),
            name: "Project".into(),
            path: "/project".into(),
        }];
        let mut sessions = vec![
            session("older", 10, Some("project"), false),
            session("latest", 20, Some("project"), false),
            session("same-time", 20, None, false),
            session("archived", 100, Some("project"), true),
            session("missing-project", 200, Some("missing"), false),
        ];
        sessions[0].pinned = true;
        assert_eq!(
            default_session_id(&sessions, &projects, |_| false).as_deref(),
            Some("latest")
        );
        assert_eq!(
            default_session_id(&sessions, &projects, |_| true).as_deref(),
            Some("same-time")
        );
        sessions.retain(|session| session.project_id.is_some());
        assert_eq!(default_session_id(&sessions, &projects, |_| true), None);
        assert_eq!(default_session_id(&[], &projects, |_| false), None);
    }

    #[test]
    fn authenticated_entry_only_exempts_its_exact_origin() {
        assert_eq!(
            normalize_connection_address(
                "http://localhost:3031/",
                true,
                Some("http://localhost:3031")
            )
            .unwrap(),
            "http://localhost:3031"
        );
        for (raw, entry) in [
            ("http://localhost:3031", None),
            ("http://localhost:3030", Some("http://localhost:3031")),
            ("http://127.0.0.1:3031", Some("http://localhost:3031")),
        ] {
            assert!(normalize_connection_address(raw, true, entry).is_err());
        }
        assert!(
            normalize_connection_address(
                "http://localhost:3031/?ticket=secret",
                true,
                Some("http://localhost:3031")
            )
            .is_err()
        );
        assert!(
            normalize_address("http://localhost:3031", true).is_err(),
            "QR policy is unchanged"
        );
    }

    #[test]
    fn history_restores_only_valid_unique_addresses_and_never_secrets() {
        let value = json!({"version":1,"recent":["https://PC.test:3030/","javascript:alert(1)","https://pc.test:3030", "https://user:secret@pc.test"],"password":"test-only"});
        let history = NodeHistory::restore(&value);
        assert_eq!(history.recent, ["https://pc.test:3030"]);
        assert_eq!(
            history.value(),
            json!({"version":1,"recent":["https://pc.test:3030"],"selected":null,"nodeIds":{}})
        );
        assert_eq!(
            NodeHistory::restore(&json!({"version":2,"recent":["https://pc.test"]})).recent,
            Vec::<String>::new()
        );
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn local_workspace_opens_without_http_and_creates_exact_nested_directory() {
        let root = std::env::temp_dir().join(format!(
            "work-ui-local-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let rpi = root.join("rpi-fixture");
        // Intentionally not executable: opening a workspace and managing projects
        // must not start a native child, nor require an HTTP service.
        std::fs::write(&rpi, "not an executable").unwrap();
        let client = WorkspaceClient::local_rpc(rpi, root.join("data")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let ConnectionOutcome::Ready(workspace) =
                open(client.clone(), Some("ignored".into())).await.unwrap()
            else {
                panic!("local workspace must never request an HTTP password");
            };
            assert!(!workspace.auth.required);
            assert!(workspace.transport.supports("projects"));
            assert!(workspace.transport.supports("files"));
            assert_eq!(workspace.sessions.len(), 0);
            let parent = root.join("chosen-parent").join("nested");
            std::fs::create_dir_all(&parent).unwrap();
            let target = parent.join("new-project");
            let project = client
                .create_project_directory("Display name", &target, false)
                .await
                .unwrap();
            assert_eq!(project.name, "Display name");
            assert!(target.is_dir());
            assert_eq!(
                std::fs::canonicalize(project.path).unwrap(),
                std::fs::canonicalize(target).unwrap()
            );
            client.shutdown().await;
        });
        drop(client);
        drop(runtime);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_history_survives_selection_and_round_trip() {
        let mut history = NodeHistory::restore(&json!({
            "version": 1,
            "recent": ["https://OLD.test/", "https://other.test"]
        }));
        assert!(history.selected.is_none());
        assert_eq!(
            startup_address(None, &history, "rpi-work://local", true),
            "https://old.test"
        );
        history.select("https://OTHER.test/");
        assert_eq!(history.selected.as_deref(), Some("https://other.test"));
        assert_eq!(history.recent, ["https://other.test", "https://old.test"]);
        let restored = NodeHistory::restore(&history.value());
        assert_eq!(restored.selected, history.selected);
        assert_eq!(restored.recent, history.recent);
    }

    #[test]
    fn stable_node_identity_collapses_address_aliases_and_round_trips() {
        let mut history = NodeHistory::default();
        history.remember_with_identity("https://pc.test:3031/", Some("node-pc"));
        history.remember_with_identity("https://pc.test:3032/", Some("node-pc"));
        history.remember_with_identity("https://other.test", Some("node-other"));

        assert_eq!(
            history.recent,
            ["https://other.test", "https://pc.test:3032"]
        );
        assert_eq!(
            history.value()["nodeIds"],
            json!({"https://other.test":"node-other","https://pc.test:3032":"node-pc"})
        );

        let restored = NodeHistory::restore(&history.value());
        assert_eq!(restored.recent, history.recent);
        assert_eq!(restored.value()["nodeIds"], history.value()["nodeIds"]);
    }

    #[test]
    fn invalid_node_identity_is_not_persisted_or_used_for_aliases() {
        let mut history = NodeHistory::default();
        history.remember_with_identity("https://safe.test", Some("node-safe"));
        for node_id in ["", "  ", "node\nunsafe"] {
            history.remember_with_identity("https://other.test", Some(node_id));
        }
        assert!(history.recent.contains(&"https://other.test".to_owned()));
        assert_eq!(
            history.value()["nodeIds"],
            json!({"https://safe.test":"node-safe"})
        );
    }

    #[test]
    fn local_selection_preserves_remote_recent_and_remote_selection_updates_both() {
        let mut history = NodeHistory::default();
        history.remember("https://remote.test/");
        history.select("rpi-work://local");
        assert_eq!(history.selected.as_deref(), Some("rpi-work://local"));
        assert_eq!(history.recent, ["https://remote.test"]);
        assert_eq!(
            startup_address(None, &history, "about:blank", true),
            "rpi-work://local"
        );
        history.remember("rpi-work://local");
        assert_eq!(history.recent, ["https://remote.test"]);
        history.select("https://NEXT.test/");
        assert_eq!(history.selected.as_deref(), Some("https://next.test"));
        assert_eq!(history.recent, ["https://next.test", "https://remote.test"]);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn local_selection_round_trips_without_entering_recent() {
        let history = NodeHistory::restore(&json!({
            "version": 1,
            "selected": "rpi-work://local",
            "recent": ["rpi-work://local", "https://remote.test"]
        }));
        assert_eq!(history.selected.as_deref(), Some("rpi-work://local"));
        let restored = NodeHistory::restore(&history.value());
        assert_eq!(restored.selected, history.selected);
        assert_eq!(restored.recent, ["https://remote.test"]);
    }

    #[test]
    fn invalid_selections_are_ignored_without_losing_history() {
        let mut history = NodeHistory::default();
        history.select("https://safe.test");
        let before = history.value();
        for address in [
            "",
            "javascript:alert(1)",
            "https://user:secret@host.test",
            "http://host.test:0",
            "rpi-work://other",
        ] {
            history.select(address);
            history.remember(address);
            assert_eq!(history.value(), before);
            let restored = NodeHistory::restore(&json!({
                "version": 1,
                "selected": address,
                "recent": [address, "https://safe.test"]
            }));
            assert!(restored.selected.is_none());
            assert_eq!(restored.recent, ["https://safe.test"]);
        }
    }

    #[test]
    fn startup_prioritizes_explicit_raw_configuration_without_invalid_fallback() {
        let mut history = NodeHistory::default();
        history.select("https://saved.test");
        for explicit in [
            "https://EXPLICIT.test/",
            "not a URL",
            "",
            "https://user:secret@host.test",
        ] {
            assert_eq!(
                startup_address(Some(explicit), &history, "rpi-work://local", true),
                explicit
            );
        }
        history.selected = Some("https://selected.test".into());
        assert_eq!(
            startup_address(None, &history, "rpi-work://local", true),
            "https://selected.test"
        );
        assert_eq!(
            startup_address(None, &NodeHistory::default(), "rpi-work://local", true),
            "rpi-work://local"
        );
    }

    #[test]
    fn remote_only_startup_never_restores_local_and_ignores_invalid_history() {
        let mut history = NodeHistory {
            selected: Some("rpi-work://local".into()),
            recent: vec!["rpi-work://local".into(), "https://remote.test/".into()],
            ..NodeHistory::default()
        };
        assert_eq!(
            startup_address(None, &history, "https://origin.test/", false),
            "https://remote.test"
        );
        history.selected = Some("javascript:bad".into());
        assert_eq!(
            startup_address(None, &history, "https://origin.test/", false),
            "https://remote.test"
        );
        history.recent.clear();
        assert_eq!(
            startup_address(None, &history, "https://origin.test/", false),
            "https://origin.test/"
        );
        assert_eq!(
            startup_address(None, &history, "rpi-work://local", false),
            "about:blank"
        );
    }

    #[test]
    fn history_is_bounded_and_moves_reconnected_node_to_front() {
        let mut history = NodeHistory::default();
        for index in 0..12 {
            history.remember(&format!("https://node-{index}.test"));
        }
        history.remember("https://node-8.test/");
        assert_eq!(history.recent.len(), 8);
        assert_eq!(history.recent[0], "https://node-8.test");
        history.remember("https://user:password@pc.test");
        assert_eq!(history.recent.len(), 8);
    }

    #[test]
    fn explicit_capabilities_gate_unsupported_commands_without_guessing_versions() {
        let capabilities = Capabilities {
            protocol: "rpi-web-v2".into(),
            pi_compatibility_target: Some("pi-rust".into()),
            supported_commands: vec!["prompt".into()],
            supported_actions: vec![],
            semantics: None,
            limits: None,
            parity_complete: false,
        };
        assert!(command_supported(&capabilities, "prompt"));
        assert!(!command_supported(&capabilities, "abort"));
        assert!(
            capability_error(&capabilities, "abort")
                .expect("unsupported command should explain downgrade")
                .contains("不支持")
        );
        let mut action_capabilities = capabilities.clone();
        action_capabilities.supported_actions = vec!["fork".into()];
        assert!(capability_error(&action_capabilities, "fork").is_none());
        assert!(capability_error(&action_capabilities, "clone").is_some());
    }

    #[test]
    fn legacy_empty_command_list_is_not_mistaken_for_explicit_deny() {
        let capabilities = Capabilities {
            protocol: "rpi-web-v1".into(),
            pi_compatibility_target: None,
            supported_commands: vec![],
            supported_actions: vec![],
            semantics: None,
            limits: None,
            parity_complete: false,
        };
        assert!(command_supported(&capabilities, "prompt"));
        assert!(capability_error(&capabilities, "prompt").is_none());
    }

    #[test]
    fn phone_rejects_legacy_numeric_loopback_and_wildcard_addresses() {
        for address in [
            "http://0x7f000001",
            "http://0177.0.0.1",
            "http://0x7f.0.0.1",
            "http://[::ffff:127.0.0.1]",
            "http://0.0.0.0",
        ] {
            assert!(
                normalize_address(address, true).is_err(),
                "accepted {address}"
            );
        }
        assert!(normalize_address("http://192.168.50.8:3030", true).is_ok());
    }
}
