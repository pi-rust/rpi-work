//! The installed rpi owns Host discovery queries and all lifecycle operations.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde_json::Value;
use tokio::{io::AsyncReadExt, process::Command};

const SETTINGS_LIMIT: u64 = 64 * 1024;
const PROFILE_LIMIT: u64 = 256 * 1024;
const OUTPUT_LIMIT: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(12);
const MAX_AREAS: usize = 64;
const AGENT_PAGE: usize = 128;

pub(super) struct Catalog {
    pub executable: PathBuf,
    pub root: PathBuf,
    pub shared_areas: Vec<String>,
    pub native: bool,
    pub control: bool,
    pub areas: Vec<Area>,
    pub extra: Option<Area>,
    pub notice: Option<&'static str>,
}

pub(super) struct Area {
    pub name: String,
    pub status: Status,
    pub agents: Vec<Agent>,
    pub default_workspace: Option<PathBuf>,
    pub can_enter: bool,
    pub incomplete: bool,
}

#[derive(Default)]
pub(super) enum Status {
    #[default]
    Uninitialized,
    Disconnected,
    Unavailable,
    Online {
        registered: u64,
        resident: u64,
        busy: u64,
        stopping: bool,
    },
}

pub(super) struct Agent {
    pub name: String,
    pub state: &'static str,
    pub target: Option<Target>,
}

#[derive(Clone)]
pub(super) struct Target {
    pub alias: String,
    pub workspace: PathBuf,
}

fn host_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.bytes().all(|value| {
            value.is_ascii_lowercase() || value.is_ascii_digit() || matches!(value, b'-' | b'_')
        })
}

fn agent_root() -> Result<PathBuf, &'static str> {
    env::var_os("RPI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".rpi/agent")))
        .filter(|path| path.is_absolute())
        .ok_or("找不到 rpi 的绝对配置目录。请检查 RPI_CODING_AGENT_DIR。")
}

fn read_json(path: &Path, limit: u64, private: bool) -> io::Result<Value> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    let (uid, forbidden_mode) = {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        // SAFETY: geteuid takes no arguments and reads the process's effective user.
        let uid = unsafe { libc::geteuid() };
        let forbidden_mode = if private { 0o077 } else { 0o022 };
        let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let metadata = fs::symlink_metadata(parent)?;
        if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & forbidden_mode != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        (uid, forbidden_mode)
    };
    #[cfg(not(unix))]
    let _ = private;
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("expected a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != uid || metadata.mode() & forbidden_mode != 0 || metadata.nlink() != 1 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("Host file exceeds its byte limit"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

async fn output(executable: &Path, args: &[&std::ffi::OsStr]) -> Option<Vec<u8>> {
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    tokio::time::timeout(TIMEOUT, async {
        let mut bytes = Vec::new();
        child
            .stdout
            .take()?
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() as u64 > OUTPUT_LIMIT || !child.wait().await.ok()?.success() {
            return None;
        }
        Some(bytes)
    })
    .await
    .ok()?
}

async fn query(executable: &Path, config: &Path, command: &str, options: &[&str]) -> Option<Value> {
    let mut args = vec![
        "host".as_ref(),
        command.as_ref(),
        "--config".as_ref(),
        config.as_os_str(),
    ];
    args.extend(options.iter().map(std::ffi::OsStr::new));
    serde_json::from_slice(&output(executable, &args).await?).ok()
}

fn workspace(profile: &Value) -> Option<PathBuf> {
    profile["agent"]["cwd"]
        .as_str()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn target_profiles(
    config: &Value,
    directory: &Path,
) -> (Option<PathBuf>, BTreeMap<String, Target>) {
    let primary = read_json(&directory.join("agent.json"), PROFILE_LIMIT, true).ok();
    let default_workspace = primary.as_ref().and_then(workspace);
    let mut targets = BTreeMap::new();
    if let Some((id, workspace)) = primary.as_ref().and_then(|profile| {
        let id = profile["agent"]["id"].as_str()?;
        (config["session_rpc_agent"].as_str() == Some(id))
            .then_some((id, default_workspace.clone()?))
    }) {
        targets.insert(
            id.to_owned(),
            Target {
                alias: "agent".into(),
                workspace,
            },
        );
    }
    if let Some(profiles) = config["named_agent_profiles"].as_array() {
        for entry in profiles.iter().take(1024) {
            let Some(alias) = entry["name"]
                .as_str()
                .filter(|name| host_name(name) && *name != "agent")
            else {
                continue;
            };
            let Some(path) = entry["path"]
                .as_str()
                .map(Path::new)
                .filter(|path| path.is_absolute())
            else {
                continue;
            };
            let Ok(profile) = read_json(path, PROFILE_LIMIT, true) else {
                continue;
            };
            if let (Some(id), Some(workspace)) =
                (profile["agent"]["id"].as_str(), workspace(&profile))
            {
                targets.entry(id.to_owned()).or_insert_with(|| Target {
                    alias: alias.into(),
                    workspace,
                });
            }
        }
    }
    (default_workspace, targets)
}

fn status(value: &Value) -> Option<Status> {
    Some(Status::Online {
        registered: value["registered"].as_u64()?,
        resident: value["resident"].as_u64()?,
        busy: value["busy"].as_u64()?,
        stopping: value["stopping"].as_bool()?,
    })
}

fn translate_state(state: &str) -> &'static str {
    match state {
        "registered" => "已登记",
        "starting" => "启动中",
        "idle" => "空闲",
        "running" => "运行中",
        "waiting_approval" => "等待审批",
        "closing" => "关闭中",
        "unloaded" => "已卸载",
        "closed" => "已关闭",
        "interrupted" => "已中断",
        "lost" => "已丢失",
        _ => "状态未知",
    }
}

async fn agent_page(
    executable: &Path,
    config: &Path,
    cursor: Option<&str>,
    targets: &BTreeMap<String, Target>,
    agents: &mut Vec<Agent>,
) -> Option<Option<String>> {
    let mut options = vec!["--limit", "128"];
    if let Some(cursor) = cursor {
        options.extend(["--after", cursor]);
    }
    let page = query(executable, config, "list", &options).await?;
    let rows = page["agents"]
        .as_array()
        .filter(|rows| rows.len() <= AGENT_PAGE)?;
    for record in rows {
        let id = record["id"].as_str()?;
        let state = record["state"].as_str().unwrap_or_default();
        agents.push(Agent {
            name: record["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(id)
                .into(),
            state: translate_state(state),
            target: if matches!(state, "idle" | "running" | "waiting_approval") {
                targets.get(id).cloned()
            } else {
                None
            },
        });
    }
    match page.get("next_cursor")? {
        Value::Null => Some(None),
        Value::String(next)
            if next.len() == 36
                && next
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
                && cursor.is_none_or(|current| next.as_str() > current) =>
        {
            Some(Some(next.clone()))
        }
        _ => None,
    }
}

async fn inspect_area(executable: &Path, name: String, config_path: PathBuf, native: bool) -> Area {
    let mut area = Area {
        name,
        status: Status::Uninitialized,
        agents: Vec::new(),
        default_workspace: None,
        can_enter: false,
        incomplete: false,
    };
    let config = match read_json(&config_path, SETTINGS_LIMIT, true) {
        Ok(config) => config,
        Err(error) if native && error.kind() == io::ErrorKind::NotFound => {
            area.can_enter = true;
            return area;
        }
        Err(_) => {
            area.status = Status::Unavailable;
            return area;
        }
    };
    let Some(directory) = config_path.parent() else {
        return area;
    };
    let observed = query(executable, &config_path, "status", &[]).await;
    area.status = observed.as_ref().and_then(status).unwrap_or_else(|| {
        let absent = config["socket_path"]
            .as_str()
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .is_some_and(|socket| {
                fs::symlink_metadata(socket)
                    .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
            });
        if absent {
            Status::Disconnected
        } else {
            Status::Unavailable
        }
    });
    let (default_workspace, targets) = if native {
        target_profiles(&config, directory)
    } else {
        (None, BTreeMap::new())
    };
    area.default_workspace = default_workspace;
    area.can_enter = area.default_workspace.is_some();
    let Status::Online { stopping, .. } = area.status else {
        return area;
    };
    if stopping {
        area.can_enter = false;
    }
    let empty = BTreeMap::new();
    let targets = if stopping { &empty } else { &targets };
    let mut cursor = None;
    for _ in 0..8 {
        match agent_page(
            executable,
            &config_path,
            cursor.as_deref(),
            targets,
            &mut area.agents,
        )
        .await
        {
            Some(None) => return area,
            Some(Some(next)) => cursor = Some(next),
            None => break,
        }
    }
    area.incomplete = true;
    area
}

async fn discover(
    executable: PathBuf,
    root: PathBuf,
    extra: Option<PathBuf>,
) -> Result<Catalog, String> {
    let help = output(&executable, &["--help".as_ref()])
        .await
        .ok_or("无法读取本机 rpi 的 Host 能力。")?;
    let help = String::from_utf8_lossy(&help);
    let shared = help
        .lines()
        .any(|line| line.trim_start().starts_with("--host ["));
    let native = cfg!(target_os = "macos")
        && help
            .lines()
            .any(|line| line.trim_start().starts_with("--native-host ["));
    let control = help
        .lines()
        .any(|line| line.trim_start().starts_with("host <command>"));
    let mut notice = None;
    let settings = match read_json(&root.join("host.json"), SETTINGS_LIMIT, false) {
        Ok(value) if value.is_object() => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Value::Null,
        _ => {
            notice = Some("无法读取 rpi 的 host.json。请检查文件格式和访问权限。");
            Value::Null
        }
    };
    let default_area = settings["defaultArea"]
        .as_str()
        .filter(|name| host_name(name))
        .unwrap_or("default")
        .to_owned();
    let mut names = BTreeSet::from([default_area.clone()]);
    if let Some(configured) = settings["areas"].as_object() {
        names.extend(configured.keys().filter(|name| host_name(name)).cloned());
    }
    let shared_areas = if shared {
        std::iter::once(default_area.clone())
            .chain(names.iter().filter(|name| **name != default_area).cloned())
            .take(MAX_AREAS)
            .collect()
    } else {
        Vec::new()
    };
    let hosts = root.join("hosts");
    if native && fs::symlink_metadata(&hosts).is_ok_and(|info| info.is_dir()) {
        if let Ok(entries) = fs::read_dir(&hosts) {
            names.extend(entries.flatten().filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                (host_name(&name) && entry.file_type().ok()?.is_dir()).then_some(name)
            }));
        }
    }
    if (native || shared) && names.len() > MAX_AREAS {
        notice = Some("此处最多显示 64 个 area。可通过配置文件查询其他 Host。");
    }
    names.remove(&default_area);
    let mut areas = Vec::new();
    let mut extra_host = None;
    let mut requests = tokio::task::JoinSet::new();
    let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let inputs = std::iter::once(default_area.clone())
        .chain(names)
        .take(MAX_AREAS)
        .filter(|_| native && control)
        .map(|name| (hosts.join(&name).join("host.json"), name, true))
        .chain(
            extra
                .filter(|_| control)
                .map(|path| (path, "手动连接的 Host".into(), false)),
        );
    for (path, name, native_area) in inputs {
        let executable = executable.clone();
        let slots = slots.clone();
        requests.spawn(async move {
            let _permit = slots
                .acquire()
                .await
                .expect("Host query limiter stays open");
            (
                native_area,
                inspect_area(&executable, name, path, native_area).await,
            )
        });
    }
    while let Some(result) = requests.join_next().await {
        let (native_area, area) = result.map_err(|_| "读取 Host 目录时发生错误。")?;
        if native_area {
            areas.push(area);
        } else {
            extra_host = Some(area);
        }
    }
    areas.sort_by(|a, b| (a.name != default_area, &a.name).cmp(&(b.name != default_area, &b.name)));
    Ok(Catalog {
        executable,
        root,
        shared_areas,
        native,
        control,
        areas,
        extra: extra_host,
        notice,
    })
}

pub(super) fn read_catalog(extra: Option<PathBuf>) -> Result<Catalog, String> {
    let executable = work_host::setup::runtime_executable()?;
    let root = agent_root()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "无法启动 Host 查询任务。")?;
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), discover(executable, root, extra))
            .await
            .map_err(|_| "读取 Host 超时。请刷新后重试。")?
    })
}

pub(super) struct Launch {
    pub executable: PathBuf,
    pub root: PathBuf,
    pub area: String,
    pub agent: Option<String>,
    pub native: bool,
}

impl Launch {
    pub fn command(&self, workspace: &Path) -> Option<String> {
        if !host_name(&self.area) || self.agent.as_ref().is_some_and(|agent| !host_name(agent)) {
            return None;
        }
        fn quote(path: &Path) -> Option<String> {
            let value = path.to_str()?;
            (!value.chars().any(char::is_control))
                .then(|| format!("'{}'", value.replace('\'', "'\\''")))
        }
        let mode = if self.native {
            "--native-host"
        } else {
            "--host"
        };
        let mut command = format!(
            "cd {} && RPI_CODING_AGENT_DIR={} {} {mode}={}",
            quote(workspace)?,
            quote(&self.root)?,
            quote(&self.executable)?,
            self.area
        );
        if let Some(agent) = &self.agent {
            command.push_str(&format!(" --agent {agent}"));
        }
        Some(command)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn open_terminal(command: &str) -> io::Result<()> {
    use std::{io::Write, os::unix::fs::PermissionsExt};

    let mut script = tempfile::Builder::new()
        .prefix("rpi-work-host-")
        .suffix(".command")
        .tempfile()?;
    writeln!(script, "#!/bin/sh\n/bin/rm -f -- \"$0\"\n{command}")?;
    script
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o700))?;
    let (_, path) = script.keep()?;
    let opened = std::process::Command::new("/usr/bin/open")
        .args(["-a", "Terminal"])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if opened {
        Ok(())
    } else {
        let _ = fs::remove_file(&path);
        Err(io::Error::other("failed to open Terminal"))
    }
}
