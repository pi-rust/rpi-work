//! Desktop composition: supervise a native rpi extension and connect a client.
//! The workspace, listeners and Agent children are owned by the extension process.
pub use rpi_node_host::{report_error, run_headless};
use std::{
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
pub mod http;
pub mod runtime;
static STARTED: AtomicBool = AtomicBool::new(false);
static CLIENT: OnceLock<work_client::WorkClient> = OnceLock::new();
static ERROR: OnceLock<String> = OnceLock::new();

#[must_use = "Retain until application shutdown"]
pub struct LocalGuard {
    process: rpi_node_launcher::Process,
}
impl LocalGuard {
    pub fn shutdown(self) {
        if let Err(error) = self.checked_shutdown() {
            eprintln!("{error}");
        }
    }
    /// Close the owned extension's stdin and await resource cleanup.
    /// # Errors
    /// Returns an error for a failing exit or unconfirmed graceful shutdown.
    pub fn checked_shutdown(mut self) -> Result<(), String> {
        self.process.stop()
    }
}

pub fn start() -> Option<LocalGuard> {
    if STARTED.swap(true, Ordering::AcqRel) {
        return None;
    }
    match launch() {
        Ok((guard, client)) => {
            let _ = CLIENT.set(client);
            Some(guard)
        }
        Err(error) => {
            let _ = ERROR.set(error);
            None
        }
    }
}
fn launch() -> Result<(LocalGuard, work_client::WorkClient), String> {
    let application = std::env::current_exe()
        .map_err(|_| "无法定位应用目录")?
        .parent()
        .ok_or("无法定位应用目录")?
        .to_owned();
    let rpi = work_node::desktop::find_runtime(
        "rpi",
        std::env::var_os("RPI_WORK_NODE_RPI_BIN")
            .or_else(|| std::env::var_os("RPI_WORK_RPI_BIN"))
            .map(PathBuf::from),
        &application,
    )
    .ok_or("未找到 rpi。请安装支持无需模型启动服务扩展的 rpi。")?;
    let selected = if std::env::var_os("RPI_WORK_NODE_EXTENSION").is_some() {
        None
    } else {
        runtime::store()?.selected()?
    };
    let activated = selected.as_ref().map(|p| p.selection.clone());
    let source = activated.as_ref().map_or_else(
        || {
            if std::env::var_os("RPI_WORK_NODE_EXTENSION").is_some() {
                "启动配置指定的扩展".into()
            } else {
                "内置或已安装的节点扩展".into()
            }
        },
        |s| format!("{} · {}", s.id, s.version),
    );
    let (library, entry) = match selected {
        Some(provider) => (provider.library, provider.selection.entry_flag),
        None => (
            extension_path(&application).ok_or(
                "未找到节点扩展。请在节点组件中下载安装运行时，或使用附带节点扩展的 Work 安装包。",
            )?,
            "http".into(),
        ),
    };
    let options = rpi_node_launcher::Launch {
        executable: rpi.clone(),
        library,
        arguments: vec![format!("--{entry}").into()],
        environment: std::collections::BTreeMap::from([(
            "RPI_WORK_RPI_BIN".into(),
            std::env::var_os("RPI_WORK_RPI_BIN").unwrap_or_else(|| rpi.into_os_string()),
        )]),
        directory: None,
    };
    let (process, control) = rpi_node_launcher::Process::start(options)?;
    let client = work_client::WorkClient::local_control(
        &control.ready.control_address,
        control.capability(),
    )
    .map_err(|_| "本机节点发布了无效的私有入口")?;
    let verify = client.clone();
    let node_id = control.ready.node_id;
    // Can be called before or inside a GUI runtime. Never block an executor on itself.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "无法验证本机节点")?;
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let identity = verify
                    .node_identity()
                    .await
                    .map_err(|_| "无法访问本机节点")?;
                if identity != node_id
                    || !verify
                        .auth()
                        .await
                        .map_err(|_| "无法验证本机节点")?
                        .authenticated
                {
                    return Err("本机节点身份或私有认证不匹配");
                }
                Ok(())
            })
            .await
            .map_err(|_| "本机节点验证超时")?
        })
    })
    .join()
    .map_err(|_| "本机节点验证失败")??;
    runtime::mark_started(activated, source);
    Ok((LocalGuard { process }, client))
}
fn extension_path(application: &std::path::Path) -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("RPI_WORK_NODE_EXTENSION") {
        return PathBuf::from(explicit)
            .canonicalize()
            .ok()
            .filter(|p| p.is_file());
    }
    let filename = format!(
        "{}rpi_http_extension{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let bundled = application.join(&filename);
    if bundled.is_file() {
        return Some(bundled);
    }
    let agent = std::env::var_os("RPI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|p| PathBuf::from(p).join(".rpi/agent"))
        })?;
    let installed = agent.join("extensions").join(filename);
    installed.is_file().then_some(installed)
}
pub fn client() -> Option<work_client::WorkspaceClient> {
    CLIENT
        .get()
        .cloned()
        .map(work_client::WorkspaceClient::Remote)
}
pub fn startup_error() -> Option<&'static str> {
    ERROR.get().map(String::as_str)
}
