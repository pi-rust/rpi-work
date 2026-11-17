//! Configuration for the HTTP entrance to the existing desktop owner.
use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use work_node_runtime::workspace::LocalWorkspace;
type NodeConnection = Arc<LocalWorkspace>;
use rpi_node::{MdnsAdvertisement, MdnsDiscoveryProvider};
use rpi_node_core::NodeIdentity;
use work_node_service::{HttpServer, ServerConfig, SharingControl};
#[path = "local_http_address.rs"]
mod address;
#[path = "http_configuration.rs"]
mod configuration;
#[path = "sharing_preference.rs"]
mod preference;
pub use work_http_component::WebAccess;
use work_http_component::{Configuration, Configure};

/// Extension startup overrides; never serialize or log the password.
#[derive(Default)]
pub struct LaunchOverrides {
    pub bind: Option<IpAddr>,
    pub port: Option<u16>,
    pub assets: Option<PathBuf>,
    pub password: Option<String>,
}

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
    pub web: WebAccess,
}
type QrIssuer = Arc<dyn Fn() -> Result<work_node_service::QrAuthorization, String> + Send + Sync>;
struct OwnedSharing {
    info: Sharing,
    issuer: Option<QrIssuer>,
    control: Option<SharingControl>,
    web_control: Option<SharingControl>,
    discovery: Option<MdnsState>,
    web_preference: PathBuf,
    preference: Option<PathBuf>,
    configuration_file: PathBuf,
    configuration: Configuration,
    restart_required: bool,
}
struct MdnsState {
    provider: Option<MdnsDiscoveryProvider>,
    advertisement: Option<MdnsAdvertisement>,
    identity: NodeIdentity,
    port: u16,
}

impl MdnsState {
    fn new(node_id: String, owner_epoch: String, port: u16) -> Self {
        Self {
            provider: None,
            advertisement: None,
            identity: NodeIdentity {
                node_id,
                owner_epoch,
            },
            port,
        }
    }

    fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled {
            if self.provider.is_none() {
                self.provider = Some(
                    MdnsDiscoveryProvider::new()
                        .map_err(|error| format!("无法启动节点发现：{error}"))?,
                );
            }
            if self.advertisement.is_none() {
                let provider = self.provider.as_ref().expect("provider initialized");
                self.advertisement = Some(
                    provider
                        .publish_http(&self.identity, self.port)
                        .map_err(|error| format!("无法发布节点发现：{error}"))?,
                );
            }
        } else if let Some(advertisement) = self.advertisement.take() {
            advertisement
                .stop()
                .map_err(|error| format!("无法停止节点发现：{error}"))?;
        }
        Ok(())
    }
}

static SHARING: OnceLock<Mutex<OwnedSharing>> = OnceLock::new();
pub fn sharing() -> Sharing {
    SHARING
        .get()
        .and_then(|s| s.lock().ok())
        .map(|s| s.info.clone())
        .unwrap_or_default()
}
pub fn configuration() -> (Option<Configuration>, bool) {
    SHARING
        .get()
        .and_then(|s| s.lock().ok())
        .map_or((None, false), |s| {
            (Some(s.configuration.clone()), s.restart_required)
        })
}
pub fn configure(request: Configure) -> Result<(), String> {
    let mut state = SHARING
        .get()
        .ok_or("节点尚未准备好")?
        .lock()
        .map_err(|_| "共享状态不可用")?;
    let password_changed = request.new_password.is_some();
    let saved = configuration::save(&state.configuration_file, request)?;
    state.restart_required |= saved != state.configuration || password_changed;
    state.configuration = saved;
    Ok(())
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
    let sharing_enabled = state.info.enabled;
    if let Some(discovery) = state.discovery.as_mut()
        && let Err(error) = discovery.set_enabled(sharing_enabled)
    {
        state.info.warning = format!("局域网节点发现不可用：{error}");
    }
    Ok(state.info.clone())
}
pub fn set_web_enabled(enabled: bool) -> Result<Sharing, String> {
    let mut state = SHARING
        .get()
        .ok_or("节点尚未准备好")?
        .lock()
        .map_err(|_| "Web 状态不可用")?;
    let control = state
        .web_control
        .clone()
        .ok_or_else(|| format!("Web 入口不可用。{}", state.info.web.status))?;
    preference::save(&state.web_preference, enabled)?;
    control.set_enabled(enabled);
    state.info.web.enabled = control.enabled();
    Ok(state.info.clone())
}

/// Explicit desktop-owner delegation; never exported as an HTTP policy handle.
pub fn issue_qr_authorization() -> Result<work_node_service::QrAuthorization, String> {
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

pub(crate) fn start(
    client: &NodeConnection,
    data: &Path,
    options: &LaunchOverrides,
) -> Vec<HttpServer> {
    let result = prepare(client, data, options);
    match result {
        Ok((servers, mut info, preference, configuration)) => {
            let control = servers
                .first()
                .filter(|_| info.listening)
                .map(HttpServer::sharing_control);
            let web_control = servers
                .last()
                .filter(|_| info.web.listening)
                .map(HttpServer::sharing_control);
            let mut discovery = servers.first().filter(|_| info.listening).map(|server| {
                MdnsState::new(
                    info.node_id.clone(),
                    client.remote_operations().owner_epoch().to_owned(),
                    server.address().port(),
                )
            });
            if let Some(state) = discovery.as_mut()
                && let Err(error) = state.set_enabled(info.enabled)
            {
                info.warning = format!("局域网节点发现不可用：{error}");
            }
            let _ = SHARING.set(Mutex::new(OwnedSharing {
                issuer: servers
                    .first()
                    .filter(|_| info.listening)
                    .map(HttpServer::qr_authorization_issuer),
                info,
                control,
                web_control,
                discovery,
                web_preference: data.join("web-sharing.json"),
                preference: Some(preference),
                configuration_file: data.join("http-component.json"),
                configuration,
                restart_required: false,
            }));
            servers
        }
        Err(error) => {
            let _ = SHARING.set(Mutex::new(OwnedSharing {
                info: Sharing {
                    status: error.clone(),
                    web: work_http_component::WebAccess {
                        status: error,
                        ..Default::default()
                    },
                    ..Sharing::default()
                },
                control: None,
                web_control: None,
                discovery: None,
                web_preference: data.join("web-sharing.json"),
                issuer: None,
                preference: None,
                configuration_file: data.join("http-component.json"),
                configuration: configuration::load(&data.join("http-component.json"))
                    .ok()
                    .flatten()
                    .map(|s| s.configuration)
                    .unwrap_or_default(),
                restart_required: false,
            }));
            Vec::new()
        }
    }
}
pub(crate) fn node_identity(data: &Path) -> Result<String, String> {
    let identity = data.join("node-id");
    if identity.exists() {
        return std::fs::read_to_string(&identity)
            .map(|id| id.trim().to_owned())
            .map_err(|_| "cannot read node identity".into());
    }
    let id = work_node_service::random_secret()?;
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
    client: &NodeConnection,
    data: &Path,
    options: &LaunchOverrides,
) -> Result<(Vec<HttpServer>, Sharing, PathBuf, Configuration), String> {
    let workspace = client;
    let saved = configuration::load(&data.join("http-component.json"))?;
    let bind = options
        .bind
        .map(|v| v.to_string())
        .or_else(|| saved.as_ref().map(|s| s.configuration.bind.clone()))
        .or_else(|| std::env::var("RPI_WORK_HTTP_BIND").ok())
        .unwrap_or_else(|| address::DEFAULT_BIND.to_string())
        .parse::<IpAddr>()
        .map_err(|_| "RPI_WORK_HTTP_BIND must be an IP address")?;
    let port = options
        .port
        .map(|v| v.to_string())
        .or_else(|| saved.as_ref().map(|s| s.configuration.port.to_string()))
        .or_else(|| std::env::var("RPI_WORK_HTTP_PORT").ok())
        .unwrap_or_else(|| "13031".into())
        .parse::<u16>()
        .map_err(|_| "invalid RPI_WORK_HTTP_PORT")?;
    let password = match options
        .password
        .clone()
        .ok_or(std::env::VarError::NotPresent)
        .or_else(|_| std::env::var("RPI_WORK_HTTP_PASSWORD"))
        .or_else(|_| std::env::var("RPI_HTTP_PASSWORD"))
    {
        Ok(value) if value.is_empty() => return Err("HTTP password must not be empty".into()),
        Ok(value) => value,
        Err(_) => work_node_service::random_secret()?,
    };
    let mut allowed_hosts: Vec<String> = std::env::var("RPI_WORK_HTTP_ALLOWED_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let public_override = match &saved {
        Some(s) => {
            (!s.configuration.public_url.is_empty()).then(|| s.configuration.public_url.clone())
        }
        None => std::env::var("RPI_WORK_HTTP_PUBLIC_URL").ok(),
    };
    let public_url = match public_override {
        Some(value) => {
            let (url, authority) = work_node_service::public_address(&value)?;
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
        None => String::new(),
    };
    let node_id = node_identity(data)?;
    let preference_file = data.join("sharing.json");
    let (enabled, preference_warning) = match preference::load(&preference_file) {
        Ok(enabled) => (enabled, String::new()),
        Err(warning) => (false, warning),
    };
    let selected_assets = options
        .assets
        .clone()
        .or_else(|| std::env::var_os("RPI_WORK_HTTP_ASSETS").map(PathBuf::from));
    let (assets, asset_error) = if selected_assets.is_some() {
        (selected_assets, None)
    } else {
        match crate::web_assets::prepare(data) {
            Ok(assets) => (assets.or_else(bundled_web_assets), None),
            Err(error) => (None, Some(error)),
        }
    };
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
    let app_address = address::suggest_app(
        (!public_url.is_empty()).then_some(public_url.as_str()),
        SocketAddr::new(bind, port),
        None,
        routed_ip,
    )?;
    if let Some(host) = &app_address.direct_host {
        allowed_hosts.push(host.clone());
    }
    if bind.is_unspecified() && bind.is_ipv4() {
        // Preserve the same-origin dev proxy and local diagnostics when the
        // default listener expands from loopback to LAN. No wildcard Hosts.
        allowed_hosts.push(format!("127.0.0.1:{port}"));
        allowed_hosts.push(format!("localhost:{port}"));
    }
    let address_hint = format!(
        "{} App 授权连接远程协作入口，浏览器使用独立的 Web 入口。",
        app_address.hint
    );
    let web_hosted = assets.is_some() || web_port.is_some();
    let web_config = ServerConfig {
        bind: SocketAddr::new(bind, port),
        password: password.clone(),
        allowed_hosts,
        node_id: node_id.clone(),
        assets: assets.clone(),
        web_proxy: web_port.is_some(),
        allow_sharing: enabled,
    };
    let mut config = web_config.clone();
    config.assets = None;
    config.web_proxy = false;
    let password_digest = saved
        .as_ref()
        .and_then(|s| s.password_digest)
        .filter(|_| options.password.is_none());
    let remote = if let Some(digest) = password_digest {
        HttpServer::start_with_password_digest(workspace.clone(), config, digest)
    } else {
        HttpServer::start(workspace.clone(), config)
    };
    let app_address = if let Ok(server) = &remote
        && port == 0
    {
        address::suggest_app(
            (!public_url.is_empty()).then_some(public_url.as_str()),
            SocketAddr::new(bind, server.address().port()),
            None,
            routed_ip,
        )?
    } else {
        app_address
    };
    let listening = remote.is_ok();
    let (status, bound_address) = match &remote {
        Ok(server) => (
            format!(
                "桌面 HTTP · {} · 与本机 RPC 共用会话和运行任务",
                server.address()
            ),
            format!("http://{}/", server.address()),
        ),
        Err(error) => (error.clone(), String::new()),
    };
    let mut servers: Vec<_> = remote.into_iter().collect();
    let web_result = match asset_error {
        Some(error) => Err(error),
        None => prepare_web(client, data, web_config, password_digest, web_port),
    };
    let web = match web_result {
        Ok((server, web)) => {
            servers.push(server);
            web
        }
        Err(status) => WebAccess {
            status,
            ..WebAccess::default()
        },
    };
    Ok((
        servers,
        Sharing {
            listening,
            enabled: listening && enabled,
            status,
            warning: preference_warning,
            public_url: app_address.url.clone(),
            app_url: app_address.url,
            address_hint,
            password,
            address: bound_address,
            node_id,
            web_hosted,
            web,
        },
        preference_file,
        Configuration {
            bind: bind.to_string(),
            port,
            public_url,
        },
    ))
}

fn bundled_web_assets() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    [parent.join("web"), parent.join("../Resources/web")]
        .into_iter()
        .find(|path| path.join("index.html").is_file())
}

fn prepare_web(
    client: &NodeConnection,
    data: &Path,
    mut config: ServerConfig,
    digest: Option<[u8; 32]>,
    proxy_port: Option<u16>,
) -> Result<(HttpServer, work_http_component::WebAccess), String> {
    let port = match std::env::var("RPI_WORK_WEB_LISTEN_PORT") {
        Ok(port) => port
            .parse::<u16>()
            .map_err(|_| "RPI_WORK_WEB_LISTEN_PORT 无效")?,
        Err(_) if config.bind.port() == 0 => 0,
        Err(_) => config
            .bind
            .port()
            .checked_add(1)
            .ok_or("Web 端口超出范围，请配置 RPI_WORK_WEB_LISTEN_PORT")?,
    };
    config.bind.set_port(port);
    // Each entrance has its own origin allowlist and authentication epoch.
    config.allowed_hosts = vec![format!("localhost:{port}"), format!("127.0.0.1:{port}")];
    let public_url = std::env::var("RPI_WORK_WEB_PUBLIC_URL").ok();
    let suggestion = address::suggest(
        public_url.as_deref(),
        config.bind,
        config.assets.is_some(),
        proxy_port,
        address::routed_lan_ipv4(),
    )?;
    if let Some(host) = &suggestion.direct_host {
        config.allowed_hosts.push(host.clone());
    }
    if let Some(url) = &public_url {
        config
            .allowed_hosts
            .push(work_node_service::public_address(url)?.1);
    }
    let (enabled, warning) = match preference::load(&data.join("web-sharing.json")) {
        Ok(value) => (value, String::new()),
        Err(error) => (false, error),
    };
    config.allow_sharing = enabled;
    let bind = config.bind;
    let server = HttpServer::start_web(client.clone(), config, digest)?;
    let url = if proxy_port.is_some() || public_url.is_some() {
        suggestion.url
    } else if bind.ip().is_loopback() {
        format!("http://{}/", server.address())
    } else if port == 0 {
        address::suggest(
            None,
            server.address(),
            true,
            None,
            address::routed_lan_ipv4(),
        )?
        .url
    } else {
        suggestion.url
    };
    Ok((
        server,
        work_http_component::WebAccess {
            listening: true,
            enabled,
            url,
            status: warning,
        },
    ))
}

/// Clear published liveness only after the executor and workspace stopped.
pub(crate) fn mark_stopped() {
    if let Some(state) = SHARING.get() {
        if let Ok(mut state) = state.lock() {
            state.info.listening = false;
            state.info.enabled = false;
            state.info.web.enabled = false;
            state.info.web.listening = false;
            state.info.status = "HTTP and workspace stopped".into();
            state.control = None;
            state.web_control = None;
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
