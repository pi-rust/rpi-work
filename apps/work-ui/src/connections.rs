use dioxus::prelude::*;
use serde_json::{Value, json};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use work_client::{AuthState, Capabilities, Project, SessionSummary, WorkClient};

#[derive(Clone, Default, PartialEq)]
pub struct NodeHistory {
    pub recent: Vec<String>,
}

impl NodeHistory {
    fn restore(value: &Value) -> Self {
        if value["version"].as_u64() != Some(1) {
            return Self::default();
        }
        let mut history = Self::default();
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
        history
    }

    pub fn remember(&mut self, address: &str) {
        if let Ok(address) = normalize_address(address, false) {
            self.recent.retain(|item| item != &address);
            self.recent.insert(0, address);
            self.recent.truncate(8);
        }
    }

    fn value(&self) -> Value {
        json!({"version":1,"recent":self.recent})
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

pub struct Workspace {
    pub auth: AuthState,
    pub capabilities: Capabilities,
    pub sessions: Vec<SessionSummary>,
    pub projects: Vec<Project>,
}

pub enum ConnectionOutcome {
    PasswordRequired(AuthState),
    Ready(Box<Workspace>),
}

pub async fn open(
    client: WorkClient,
    password: Option<String>,
) -> Result<ConnectionOutcome, String> {
    with_timeout(async move {
        let auth = if let Some(password) = password {
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
        let sessions = client
            .sessions()
            .await
            .map_err(|error| format!("会话加载失败：{error}"))?;
        let projects = client
            .projects()
            .await
            .map_err(|error| format!("项目加载失败：{error}"))?;
        Ok(ConnectionOutcome::Ready(Box::new(Workspace {
            auth,
            capabilities,
            sessions,
            projects,
        })))
    })
    .await?
}

async fn with_timeout<T>(future: impl Future<Output = T>) -> Result<T, String> {
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

pub fn normalize_address(raw: &str, phone: bool) -> Result<String, String> {
    let value = raw.trim();
    if value.len() > 2048 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("服务地址过长或包含无效字符。".into());
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err("请输入完整的 HTTP(S) 服务地址，例如 https://电脑地址:3030。".into());
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
    fn history_restores_only_valid_unique_addresses_and_never_secrets() {
        let value = json!({"version":1,"recent":["https://PC.test:3030/","javascript:alert(1)","https://pc.test:3030", "https://user:secret@pc.test"],"password":"test-only"});
        let history = NodeHistory::restore(&value);
        assert_eq!(history.recent, ["https://pc.test:3030"]);
        assert_eq!(
            history.value(),
            json!({"version":1,"recent":["https://pc.test:3030"]})
        );
        assert_eq!(
            NodeHistory::restore(&json!({"version":2,"recent":["https://pc.test"]})).recent,
            Vec::<String>::new()
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
