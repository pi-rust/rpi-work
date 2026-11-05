//! Configuration for the HTTP entrance to the existing desktop owner.
use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use work_client::WorkspaceClient;
use work_server::{HttpServer, ServerConfig, SharingControl};
#[path = "local_http_address.rs"]
mod address;
#[path = "sharing_preference.rs"]
mod preference;

#[derive(Clone, Default)]
pub struct Sharing {
    pub listening: bool,
    pub enabled: bool,
    pub status: String,
    pub warning: String,
    pub public_url: String,
    /// Authenticated API entrance for native Android; does not require Web assets.
    pub app_url: String,
    pub address_hint: String,
    pub password: String,
    /// Actual bound HTTP origin (including the OS-selected port), not a QR suggestion.
    pub address: String,
    /// Persistent identity from the owner's data directory.
    pub node_id: String,
    /// Static assets or an explicitly configured same-origin Web proxy.
    pub web_hosted: bool,
}
type QrIssuer = Arc<dyn Fn() -> Result<work_server::QrAuthorization, String> + Send + Sync>;
struct OwnedSharing {
    info: Sharing,
    issuer: Option<QrIssuer>,
    control: Option<SharingControl>,
    preference: Option<PathBuf>,
}
static SHARING: OnceLock<Mutex<OwnedSharing>> = OnceLock::new();
pub fn sharing() -> Sharing {
    SHARING
        .get()
        .and_then(|s| s.lock().ok())
        .map(|s| s.info.clone())
        .unwrap_or_default()
}
/// The owner-only action. Remote/Web clients cannot change this policy.
pub fn set_shared(enabled: bool) -> Result<Sharing, String> {
    let mut state = SHARING
        .get()
        .ok_or("桌面 HTTP 尚未准备好")?
        .lock()
        .map_err(|_| "共享状态不可用")?;
    let control = state
        .control
        .clone()
        .ok_or("桌面 HTTP 入口不可用，请检查监听配置")?;
    let path = state.preference.as_ref().ok_or("共享设置不可用")?;
    preference::save(path, enabled)?;
    control.set_enabled(enabled);
    state.info.enabled = control.enabled();
    state.info.warning.clear();
    Ok(state.info.clone())
}

/// Explicit desktop-owner delegation; never exported as an HTTP policy handle.
pub fn issue_qr_authorization() -> Result<work_server::QrAuthorization, String> {
    let state = SHARING
        .get()
        .ok_or("桌面 HTTP 尚未准备好")?
        .lock()
        .map_err(|_| "共享状态不可用")?;
    if !state.info.listening || !state.info.enabled {
        return Err("请先开启这台电脑的共享".into());
    }
    state.issuer.as_ref().ok_or("桌面授权入口不可用")?()
}

/// Credential-free native LAN probe. `WebView` fetch cannot bypass Origin policy.
/// No redirects, cookies, passwords, tickets or response bodies enter errors.
pub async fn authorization_endpoint_node(url: &str) -> Result<String, String> {
    let (url, _) = work_server::public_address(url)?;
    // A fresh transport has no owner/remote credentials. Native rustls handles
    // HTTPS as well as LAN HTTP; redirects are disabled by WorkClient.
    let client = work_client::WorkClient::new(&url).map_err(|_| "无法验证分享地址。".to_owned())?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.qr_authorization_node(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .flatten()
    .ok_or_else(|| "分享地址不可达、共享已关闭或节点无法验证。".into())
}

pub(crate) fn start(client: &WorkspaceClient, data: &Path) -> Option<HttpServer> {
    let result = prepare(client, data);
    match result {
        Ok((server, info, preference)) => {
            let control = Some(server.sharing_control());
            let _ = SHARING.set(Mutex::new(OwnedSharing {
                issuer: Some(server.qr_authorization_issuer()),
                info,
                control,
                preference: Some(preference),
            }));
            Some(server)
        }
        Err(error) => {
            let _ = SHARING.set(Mutex::new(OwnedSharing {
                info: Sharing {
                    status: error,
                    ..Sharing::default()
                },
                control: None,
                issuer: None,
                preference: None,
            }));
            None
        }
    }
}
fn node_identity(data: &Path) -> Result<String, String> {
    let identity = data.join("node-id");
    if identity.exists() {
        return std::fs::read_to_string(&identity)
            .map(|id| id.trim().to_owned())
            .map_err(|_| "cannot read node identity".into());
    }
    let id = work_server::random_secret()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&identity)
        .map_err(|_| "cannot create node identity")?;
    file.write_all(id.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| "cannot save node identity")?;
    Ok(id)
}

#[allow(clippy::too_many_lines)] // Validate configuration before publishing a single owner listener.
fn prepare(
    client: &WorkspaceClient,
    data: &Path,
) -> Result<(HttpServer, Sharing, PathBuf), String> {
    let WorkspaceClient::Local(workspace) = client else {
        return Err("HTTP sharing requires the desktop owner".into());
    };
    let bind = std::env::var("RPI_WORK_HTTP_BIND")
        .unwrap_or_else(|_| address::DEFAULT_BIND.to_string())
        .parse::<IpAddr>()
        .map_err(|_| "RPI_WORK_HTTP_BIND must be an IP address")?;
    let port = std::env::var("RPI_WORK_HTTP_PORT")
        .unwrap_or_else(|_| "13031".into())
        .parse::<u16>()
        .map_err(|_| "invalid RPI_WORK_HTTP_PORT")?;
    let password = match std::env::var("RPI_WORK_HTTP_PASSWORD")
        .or_else(|_| std::env::var("RPI_HTTP_PASSWORD"))
    {
        Ok(value) if value.is_empty() => return Err("HTTP password must not be empty".into()),
        Ok(value) => value,
        Err(_) => work_server::random_secret()?,
    };
    let mut allowed_hosts: Vec<String> = std::env::var("RPI_WORK_HTTP_ALLOWED_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let public_url = match std::env::var("RPI_WORK_HTTP_PUBLIC_URL") {
        Ok(value) => {
            let (url, authority) = work_server::public_address(&value)?;
            if bind.is_loopback()
                && std::env::var("RPI_WORK_HTTP_ALLOWED_HOSTS")
                    .unwrap_or_default()
                    .is_empty()
            {
                return Err("局域网分享需显式配置监听地址，或为同源反向代理配置 RPI_WORK_HTTP_ALLOWED_HOSTS".into());
            }
            allowed_hosts.push(authority);
            url
        }
        Err(_) => String::new(),
    };
    let node_id = node_identity(data)?;
    let preference_file = data.join("sharing.json");
    let (enabled, preference_warning) = match preference::load(&preference_file) {
        Ok(enabled) => (enabled, String::new()),
        Err(warning) => (false, warning),
    };
    let assets = std::env::var_os("RPI_WORK_HTTP_ASSETS").map(std::path::PathBuf::from);
    let web_port = match std::env::var("RPI_WORK_HTTP_WEB_PORT") {
        Ok(value) if !value.trim().is_empty() => Some(
            value
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or("RPI_WORK_HTTP_WEB_PORT must be a nonzero port")?,
        ),
        _ => None,
    };
    let routed_ip = address::routed_lan_ipv4();
    let suggestion = address::suggest(
        (!public_url.is_empty()).then_some(public_url.as_str()),
        SocketAddr::new(bind, port),
        assets.is_some(),
        web_port,
        routed_ip,
    )?;
    let app_address = address::suggest_app(
        (!public_url.is_empty()).then_some(public_url.as_str()),
        SocketAddr::new(bind, port),
        web_port,
        routed_ip,
    )?;
    for host in [&suggestion.direct_host, &app_address.direct_host]
        .into_iter()
        .flatten()
    {
        allowed_hosts.push(host.clone());
    }
    if bind.is_unspecified() && bind.is_ipv4() {
        // Preserve the same-origin dev proxy and local diagnostics when the
        // default listener expands from loopback to LAN. No wildcard Hosts.
        allowed_hosts.push(format!("127.0.0.1:{port}"));
        allowed_hosts.push(format!("localhost:{port}"));
    }
    let address_hint = if !app_address.url.is_empty() && suggestion.url.is_empty() {
        "已提供局域网 App 授权入口。使用 Android App 扫描一次性授权码；浏览器入口需另配 Web 资源或同源代理。不会自动修改防火墙，多网卡可显式配置分享地址。".into()
    } else {
        suggestion.hint
    };
    let web_hosted = assets.is_some() || web_port.is_some();
    let server = HttpServer::start(
        workspace.node_owner(),
        ServerConfig {
            bind: SocketAddr::new(bind, port),
            password: password.clone(),
            allowed_hosts,
            node_id: node_id.clone(),
            assets,
            web_proxy: web_port.is_some(),
            allow_sharing: enabled,
        },
    )?;
    let status = format!(
        "桌面 HTTP · {} · 与本机 RPC 共用会话和运行任务",
        server.address()
    );
    let bound_address = format!("http://{}/", server.address());
    Ok((
        server,
        Sharing {
            listening: true,
            enabled,
            status,
            warning: preference_warning,
            public_url: suggestion.url,
            app_url: app_address.url,
            address_hint,
            password,
            address: bound_address,
            node_id,
            web_hosted,
        },
        preference_file,
    ))
}

/// Clear published liveness only after the executor and workspace stopped.
pub(crate) fn mark_stopped() {
    if let Some(state) = SHARING.get() {
        if let Ok(mut state) = state.lock() {
            state.info.listening = false;
            state.info.enabled = false;
            state.info.status = "HTTP and workspace stopped".into();
            state.control = None;
            state.issuer = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_identity_is_persisted_without_an_agent_or_global_environment() {
        let root = tempfile::tempdir().unwrap();
        let first = node_identity(root.path()).unwrap();
        assert_ne!(first, "");
        assert_eq!(node_identity(root.path()).unwrap(), first);
        assert_eq!(
            std::fs::read_to_string(root.path().join("node-id")).unwrap(),
            first
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn stored_node_identity_is_reused_and_unreadable_identity_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let identity = root.path().join("node-id");
        std::fs::write(&identity, "existing-id\n").unwrap();
        assert_eq!(node_identity(root.path()).unwrap(), "existing-id");
        assert_eq!(std::fs::read_to_string(&identity).unwrap(), "existing-id\n");
        std::fs::remove_file(&identity).unwrap();
        std::fs::create_dir(&identity).unwrap();
        assert!(node_identity(root.path()).is_err());
        assert!(identity.is_dir());
    }
}
