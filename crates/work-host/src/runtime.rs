//! Native package composition. Download/staging lives in rpi-node-launcher.
pub use rpi_node_launcher::packages::Selection;
use std::path::PathBuf;
use std::sync::OnceLock;
#[derive(Clone)]
struct Activation {
    selection: Option<Selection>,
    source: String,
}
static ACTIVE: OnceLock<Activation> = OnceLock::new();
pub struct Status {
    /// Current private entrance is reachable; persisted files never prove liveness.
    pub running: bool,
    pub current: String,
    pub selected: Option<Selection>,
    pub restart_required: bool,
    pub explicit_override: bool,
}
pub(crate) fn mark_started(selection: Option<Selection>, source: String) {
    let _ = ACTIVE.set(Activation { selection, source });
}
/// Read selected metadata separately from the verified entrance's liveness.
/// # Errors
/// Returns an error when the cached provider selection cannot be read or validated.
pub async fn status() -> Result<Status, String> {
    let selected = tokio::task::spawn_blocking(|| {
        store()?
            .selected()
            .map(|provider| provider.map(|p| p.selection))
    })
    .await
    .map_err(|_| "无法读取运行时状态")??;
    let running = if let Some(client) = crate::client() {
        tokio::time::timeout(std::time::Duration::from_secs(3), client.health())
            .await
            .is_ok_and(|r| r.is_ok())
    } else {
        false
    };
    let active = ACTIVE.get();
    let current = if running {
        active.map_or_else(|| "默认节点运行时".into(), |a| a.source.clone())
    } else {
        active.map_or_else(
            || "尚未启动".into(),
            |a| format!("节点入口不可用（上次启动：{}）", a.source),
        )
    };
    let restart_required = selected.as_ref() != active.and_then(|a| a.selection.as_ref())
        || (!running && selected.is_some());
    Ok(Status {
        running,
        current,
        selected,
        restart_required,
        explicit_override: std::env::var_os("RPI_WORK_NODE_EXTENSION").is_some(),
    })
}
/// Select the default for next startup without touching loaded files.
/// # Errors
/// Returns an error if the selection file cannot be removed.
pub async fn restore_default() -> Result<(), String> {
    tokio::task::spawn_blocking(|| store()?.clear_selection())
        .await
        .map_err(|_| "无法恢复默认运行时")?
}
/// Resolve the parent-side native package cache without creating a node owner.
/// # Errors
/// Returns an error when the application data directory cannot be resolved.
pub fn store() -> Result<rpi_node_launcher::packages::Store, String> {
    let directory = std::env::var_os("RPI_WORK_COMPONENTS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) {
                "LOCALAPPDATA"
            } else {
                "HOME"
            })
            .map(|p| {
                PathBuf::from(p).join(if cfg!(windows) {
                    "rpi-work/components"
                } else {
                    ".local/share/rpi-work/components"
                })
            })
        })
        .ok_or("无法定位本机组件目录")?;
    Ok(rpi_node_launcher::packages::Store::new(directory))
}
/// Download and stage explicitly trusted native code for next startup.
/// # Errors
/// Rejects explicit launch overrides, untrusted/invalid packages, and failed downloads or writes.
pub async fn install(
    bundle: rpi_components::Bundle,
    allow_native_code: bool,
) -> Result<Selection, String> {
    if std::env::var_os("RPI_WORK_NODE_EXTENSION").is_some() {
        return Err("当前启动配置指定了节点扩展路径，请取消该覆盖后再选择下载的运行时".into());
    }
    store()?.install(bundle, allow_native_code).await
}
/// Read and verify the next-startup provider.
/// # Errors
/// Returns an error for invalid, missing or corrupt selected package files.
pub fn selected() -> Result<Option<Selection>, String> {
    Ok(store()?.selected()?.map(|p| p.selection))
}
