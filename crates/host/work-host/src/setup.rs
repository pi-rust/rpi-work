//! Desktop runtime preparation, owned by the launch stack through final shutdown.
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::watch,
};
use work_update::native::rpi_runtime::{self, RuntimeInstallProgress};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Checking,
    NeedsRuntime,
    Resolving,
    Downloading,
    Verifying,
    Installing,
    Starting,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub message: String,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub received: u64,
    pub total: u64,
    pub can_install: bool,
}

impl Status {
    #[must_use]
    pub fn busy(&self) -> bool {
        !matches!(
            self.phase,
            Phase::NeedsRuntime | Phase::Ready | Phase::Failed | Phase::Cancelled
        )
    }

    #[must_use]
    pub fn can_cancel(&self) -> bool {
        self.busy() && self.phase != Phase::Starting
    }
}

#[derive(Clone, Debug)]
pub enum Selection {
    Automatic,
    Managed,
    File(PathBuf),
}

#[derive(Default)]
struct Control {
    worker: Option<JoinHandle<()>>,
    cancel: Option<watch::Sender<bool>>,
    guard: Option<crate::LocalGuard>,
    closed: bool,
}

pub struct DesktopSetup {
    status: watch::Sender<Status>,
    control: Mutex<Control>,
}

#[derive(Debug)]
struct Failure {
    message: String,
    can_install: bool,
    phase: Phase,
}

impl Failure {
    fn runtime(message: String) -> Self {
        Self {
            message,
            can_install: true,
            phase: Phase::Failed,
        }
    }

    fn startup(message: String) -> Self {
        Self {
            message,
            can_install: false,
            phase: Phase::Failed,
        }
    }

    fn from_startup(error: crate::StartupFailure) -> Self {
        Self {
            message: error.message,
            can_install: error.can_install,
            phase: Phase::Failed,
        }
    }

    fn missing_runtime() -> Self {
        Self {
            message: "没有找到 rpi。请选择下载并安装，或指定已有的 rpi 路径。".into(),
            can_install: true,
            phase: Phase::NeedsRuntime,
        }
    }

    fn cancelled() -> Self {
        Self {
            message: "已取消准备运行环境。可以重新下载，或指定已有的 rpi 路径。".into(),
            can_install: true,
            phase: Phase::Cancelled,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Preference {
    path: PathBuf,
    // An explicit user choice may replace this particular invalid environment
    // override. A later changed environment override takes precedence again.
    replaced_override: Option<PathBuf>,
}

impl DesktopSetup {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            status: watch::channel(Status {
                message: "正在检查 rpi 运行环境…".into(),
                ..Status::default()
            })
            .0,
            control: Mutex::new(Control::default()),
        })
    }

    pub fn snapshot(&self) -> Status {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Status> {
        self.status.subscribe()
    }

    pub fn begin(self: &Arc<Self>, selection: Selection) {
        let mut control = self
            .control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if control.closed || control.cancel.is_some() || control.guard.is_some() {
            return;
        }
        let (cancel, receiver) = watch::channel(false);
        control.cancel = Some(cancel);
        self.status.send_replace(Status {
            message: "正在检查 rpi 运行环境…".into(),
            ..Status::default()
        });
        let setup = Arc::clone(self);
        control.worker = Some(std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| Failure::startup(format!("无法创建运行环境准备任务：{error}")))
                .and_then(|runtime| runtime.block_on(setup.prepare(selection, receiver)));
            let mut control = setup
                .control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            control.cancel = None;
            match result {
                Ok(guard) if !control.closed => {
                    control.guard = Some(guard);
                    setup.update(Phase::Ready, "本机运行环境已就绪");
                }
                Ok(guard) => {
                    drop(control);
                    guard.shutdown();
                }
                Err(error) if !control.closed => {
                    let mut status = setup.snapshot();
                    status.phase = error.phase;
                    status.message = error.message;
                    status.can_install = error.can_install;
                    setup.status.send_replace(status);
                }
                Err(_) => {}
            }
        }));
    }

    pub fn cancel(&self) {
        let control = self
            .control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.snapshot().can_cancel()
            && let Some(cancel) = &control.cancel
        {
            cancel.send_replace(true);
        }
    }

    /// The launch stack calls this before its event loop exits. A worker racing
    /// shutdown retains and shuts down its own guard instead of publishing it.
    pub fn shutdown(&self) {
        let (worker, guard) = {
            let mut control = self
                .control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            control.closed = true;
            if let Some(cancel) = &control.cancel {
                cancel.send_replace(true);
            }
            (control.worker.take(), control.guard.take())
        };
        if let Some(worker) = worker {
            let _ = worker.join();
        }
        if let Some(guard) = guard {
            guard.shutdown();
        }
    }

    fn update(&self, phase: Phase, message: &str) {
        let mut status = self.snapshot();
        status.phase = phase;
        status.message = message.into();
        self.status.send_replace(status);
    }

    async fn prepare(
        &self,
        selection: Selection,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<crate::LocalGuard, Failure> {
        let mut prepared = None;
        let path = if matches!(selection, Selection::Managed) {
            let mut retry = 0u32;
            loop {
                ensure_running(&cancel)?;
                let result = tokio::select! {
                    biased;
                    _ = cancel.changed() => return Err(Failure::cancelled()),
                    result = rpi_runtime::prepare_latest(|event| self.install_progress(event, retry)) => result,
                };
                match result {
                    Ok(runtime) => {
                        let path = runtime.executable().to_owned();
                        prepared = Some(runtime);
                        break path;
                    }
                    Err(error) if error.retryable && retry < 2 => {
                        retry += 1;
                        self.update(
                            Phase::Resolving,
                            &format!("网络暂时不可用，正在进行第 {retry}/2 次重试…"),
                        );
                        tokio::select! {
                            biased;
                            _ = cancel.changed() => return Err(Failure::cancelled()),
                            () = tokio::time::sleep(Duration::from_secs(u64::from(retry) * 2)) => {}
                        }
                    }
                    Err(error) => return Err(Failure::runtime(error.to_string())),
                }
            }
        } else {
            resolve(&selection)
                .map_err(Failure::runtime)?
                .ok_or_else(Failure::missing_runtime)?
        };
        ensure_running(&cancel)?;
        let path = std::fs::canonicalize(path)
            .map_err(|error| Failure::runtime(format!("无法定位 rpi 程序：{error}")))?;
        let mut status = self.snapshot();
        status.path = prepared.is_none().then_some(path.clone());
        status.phase = Phase::Checking;
        status.message = "正在验证 rpi 版本与 RPC 能力…".into();
        self.status.send_replace(status);
        let version = probe(&path, &mut cancel).await?;
        if prepared
            .as_ref()
            .is_some_and(|runtime| runtime.release.version != version)
        {
            return Err(Failure::runtime(
                "下载程序的版本与发布信息不一致，原有版本已保留。".into(),
            ));
        }
        {
            // Serialize the final cancellation check with cancel() before activation.
            let _control = self
                .control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            ensure_running(&cancel)?;
            self.update(Phase::Starting, "正在启动本机工作空间…");
        }
        let path = if let Some(runtime) = prepared {
            runtime.activate().map_err(Failure::runtime)?
        } else {
            path
        };
        let mut status = self.snapshot();
        status.path = Some(path.clone());
        status.version = Some(version);
        self.status.send_replace(status);
        ensure_running(&cancel)?;
        let guard = crate::try_start_with(crate::LaunchOverrides {
            rpi_bin: Some(path.clone()),
            ..Default::default()
        })
        .map_err(Failure::from_startup)?;
        if !matches!(selection, Selection::Automatic) {
            // Failure to save a preference must never discard a live owner.
            if let Err(error) = save_preference(&path) {
                eprintln!("Unable to save rpi selection: {error}");
            }
        }
        Ok(guard)
    }

    fn install_progress(&self, event: RuntimeInstallProgress, retry: u32) {
        let mut status = self.snapshot();
        (status.phase, status.message) = match event {
            RuntimeInstallProgress::Resolving => {
                (Phase::Resolving, "正在获取 rpi 安装包信息…".into())
            }
            RuntimeInstallProgress::Downloading { received, total } => {
                status.received = received;
                status.total = total;
                (
                    Phase::Downloading,
                    if retry == 0 {
                        "正在下载 rpi…".into()
                    } else {
                        format!("正在重新下载 rpi（第 {retry}/2 次重试）…")
                    },
                )
            }
            RuntimeInstallProgress::Verifying => (Phase::Verifying, "正在校验下载文件…".into()),
            RuntimeInstallProgress::Extracting => (Phase::Installing, "正在解压 rpi…".into()),
        };
        self.status.send_replace(status);
    }
}

fn ensure_running(cancel: &watch::Receiver<bool>) -> Result<(), Failure> {
    if *cancel.borrow() {
        Err(Failure::cancelled())
    } else {
        Ok(())
    }
}

fn preference_path() -> Result<PathBuf, String> {
    Ok(rpi_runtime::install_dir()?.join("selection.json"))
}

fn save_preference(path: &Path) -> Result<(), String> {
    use std::io::Write;
    let destination = preference_path()?;
    let directory = destination.parent().ok_or("无法定位运行环境配置目录")?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let preference = Preference {
        path: path.to_owned(),
        replaced_override: std::env::var_os("RPI_WORK_RPI_BIN").map(PathBuf::from),
    };
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec(&preference).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(destination)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Resolve the active desktop runtime or look up the configured local installation.
pub fn runtime_executable() -> Result<PathBuf, String> {
    if let Some(owner) = work_node_host::client() {
        return std::fs::canonicalize(owner.runtime_executable())
            .map_err(|_| "无法定位工作台使用的 rpi 程序。".into());
    }
    resolve(&Selection::Automatic)?
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| "找不到本机 rpi 程序。".into())
}

fn resolve(selection: &Selection) -> Result<Option<PathBuf>, String> {
    let path = match selection {
        Selection::File(path) if path.is_dir() => {
            Some(path.join(if cfg!(windows) { "rpi.exe" } else { "rpi" }))
        }
        Selection::File(path) => Some(path.clone()),
        Selection::Managed => return Ok(None),
        Selection::Automatic => {
            let explicit = std::env::var_os("RPI_WORK_RPI_BIN").map(PathBuf::from);
            let saved = preference_path()
                .ok()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice::<Preference>(&bytes).ok());
            if let Some(saved) =
                saved.filter(|saved| explicit.is_none() || saved.replaced_override == explicit)
            {
                Some(saved.path)
            } else if explicit.is_some() {
                explicit
            } else {
                let application = std::env::current_exe().map_err(|error| error.to_string())?;
                return Ok(work_node_runtime::desktop::find_runtime(
                    "rpi",
                    None,
                    application.parent().ok_or("无法定位应用目录")?,
                ));
            }
        }
    };
    path.map(|path| {
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("找不到配置的 rpi 程序：{}", path.display()))
        }
    })
    .transpose()
}

fn probe_command(path: &Path, directory: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .current_dir(directory)
        .env("RPI_CODING_AGENT_DIR", directory.join("agent"))
        .env("RPI_WORK_DATA_DIR", directory.join("work"))
        .env("RPI_OFFLINE", "1")
        .env_remove("RPI_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

async fn probe(path: &Path, cancel: &mut watch::Receiver<bool>) -> Result<String, Failure> {
    let directory = tempfile::tempdir().map_err(|error| Failure::runtime(error.to_string()))?;
    // rpi resolves a model before publishing RPC readiness. An isolated local
    // provider allows this handshake before the user's first model configuration.
    // The offline probe only reads readiness and then terminates the child.
    let agent = directory.path().join("agent");
    std::fs::create_dir(&agent).map_err(|error| Failure::runtime(error.to_string()))?;
    let models = serde_json::json!({
        "providers": {
            "work-runtime-check": {
                "api": "openai-completions",
                "baseUrl": "http://127.0.0.1:1",
                "apiKey": "runtime-handshake-only",
                "models": [{
                    "id": "handshake",
                    "name": "Runtime handshake",
                    "reasoning": false,
                    "input": ["text"],
                    "contextWindow": 4096,
                    "maxTokens": 16,
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
                }]
            }
        }
    });
    std::fs::write(agent.join("models.json"), models.to_string())
        .map_err(|error| Failure::runtime(error.to_string()))?;
    let path = std::fs::canonicalize(path).map_err(|error| Failure::runtime(error.to_string()))?;
    let mut child = probe_command(&path, directory.path())
        .arg("--version")
        .spawn()
        .map_err(|error| Failure::runtime(format!("无法执行 rpi：{error}")))?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| Failure::runtime("无法读取 rpi 版本".into()))?
        .take(4096);
    let mut bytes = Vec::new();
    let version = tokio::select! {
        biased;
        _ = cancel.changed() => Err(Failure::cancelled()),
        result = tokio::time::timeout(Duration::from_secs(10), async {
            output.read_to_end(&mut bytes).await.map_err(|error| Failure::runtime(error.to_string()))?;
            let status = child.wait().await.map_err(|error| Failure::runtime(error.to_string()))?;
            if !status.success() { return Err(Failure::runtime("rpi 版本检查失败，请选择可用的程序。".into())); }
            parse_version(&bytes).map_err(Failure::runtime)
        }) => result.unwrap_or_else(|_| Err(Failure::runtime("rpi 版本检查超时".into()))),
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    let version = version?;
    ensure_running(cancel)?;
    let mut child = probe_command(&path, directory.path())
        .args([
            "--mode",
            "rpc",
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-context-files",
            "--no-themes",
            "--provider",
            "work-runtime-check",
            "--model",
            "handshake",
        ])
        .spawn()
        .map_err(|error| Failure::runtime(format!("无法启动 rpi RPC：{error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Failure::runtime("无法读取 rpi RPC".into()))?;
    let mut reader = BufReader::new(stdout.take(1_048_576));
    let mut line = Vec::new();
    let result = tokio::select! {
        biased;
        _ = cancel.changed() => Err(Failure::cancelled()),
        result = tokio::time::timeout(Duration::from_secs(15), reader.read_until(b'\n', &mut line)) => {
            match result {
                Ok(Ok(_)) => validate_ready(&line).map_err(Failure::runtime),
                Ok(Err(error)) => Err(Failure::runtime(format!("rpi RPC 读取失败：{error}"))),
                Err(_) => Err(Failure::runtime("rpi RPC 就绪检查超时".into())),
            }
        }
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    result?;
    Ok(version)
}

fn parse_version(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "rpi 版本输出无效")?;
    // Require the program identity as well as a semver to reject unrelated CLIs.
    if !text
        .split_whitespace()
        .any(|word| word.eq_ignore_ascii_case("rpi"))
    {
        return Err("所选程序未报告 rpi 版本，请选择正确的 rpi 可执行文件。".into());
    }
    text.split_whitespace()
        .find_map(|word| semver::Version::parse(word.trim_start_matches('v')).ok())
        .map(|version| version.to_string())
        .ok_or_else(|| "无法识别 rpi 版本".into())
}

fn validate_ready(line: &[u8]) -> Result<(), String> {
    let ready: serde_json::Value =
        serde_json::from_slice(line).map_err(|_| "rpi 未返回有效的 RPC 就绪信息")?;
    let commands = ready["supportedCommands"].as_array();
    if ready["type"] != "ready"
        || ready["protocol"] != "rpi"
        || !["prompt", "get_state"].iter().all(|required| {
            commands.is_some_and(|commands| commands.iter().any(|command| command == required))
        })
    {
        return Err("rpi 版本缺少工作台需要的 RPC 能力，请选择兼容版本。".into());
    }
    Ok(())
}
