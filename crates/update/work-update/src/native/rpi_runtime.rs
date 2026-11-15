//! Download and install the native `rpi` runtime from its fixed GitHub release.

use super::sha256;
use fs2::FileExt;
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const REPOSITORY: &str = "bigfish1913/pi-rust";
const RELEASES_URL: &str = "https://github.com/bigfish1913/pi-rust/releases";
const MAX_METADATA: usize = 512 * 1024;
const MAX_CHECKSUM: usize = 4096;
const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_RUNTIME: u64 = 256 * 1024 * 1024;

fn http_client() -> Result<reqwest::Client, String> {
    crate::http_client_builder()
        .http1_only()
        .read_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeRelease {
    pub version: String,
    pub target: String,
    archive_name: String,
    archive_size: u64,
    archive_sha256: String,
    archive_url: String,
    checksum_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    digest: Option<String>,
}

/// Return the latest stable rpi release for this desktop's target.
pub async fn latest() -> Result<RuntimeRelease, String> {
    release_metadata().await.map_err(|error| error.to_string())
}

async fn release_metadata() -> Result<RuntimeRelease, RuntimeInstallError> {
    let target = super::super::target();
    let extension = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    if target == "unsupported" || target.contains("android") || target.contains("wasm") {
        return Err("当前平台没有可用的 rpi 下载包".into());
    }

    let client = http_client()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| RuntimeInstallError::http("无法连接 GitHub", error))?
        .error_for_status()
        .map_err(|error| RuntimeInstallError::http("GitHub 发布信息读取失败", error))?;
    let bytes = read_bounded(response, MAX_METADATA).await?;
    let metadata: GitHubRelease = serde_json::from_slice(&bytes)
        .map_err(|error| format!("GitHub 发布信息格式无效：{error}"))?;
    if metadata.draft || metadata.prerelease {
        return Err("GitHub 没有可用的稳定 rpi 版本".into());
    }
    let version = metadata
        .tag_name
        .strip_prefix('v')
        .ok_or("GitHub rpi 版本标签无效")?;
    let version = semver::Version::parse(version).map_err(|_| "GitHub rpi 版本号无效")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("GitHub rpi 版本不是稳定版本".into());
    }
    let version = version.to_string();
    let archive_name = format!("rpi-v{version}-{target}.{extension}");
    let checksum_name = format!("{archive_name}.sha256");
    let archive = metadata
        .assets
        .iter()
        .find(|asset| asset.name == archive_name)
        .ok_or("GitHub 发布缺少当前平台的 rpi 下载包")?;
    let checksum = metadata
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .ok_or("GitHub 发布缺少 rpi 校验文件")?;
    if archive.size == 0 || archive.size > MAX_ARCHIVE {
        return Err("rpi 下载包大小无效或超过限制".into());
    }
    let archive_sha256 = archive
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("GitHub rpi 下载包缺少 SHA-256 摘要")?
        .to_ascii_lowercase();
    let fixed_url = |name: &str| format!("{RELEASES_URL}/download/{}/{}", metadata.tag_name, name);
    // Use fixed repository URLs rather than forwarding arbitrary URLs from API metadata.
    Ok(RuntimeRelease {
        version,
        target: target.into(),
        archive_name,
        archive_size: archive.size,
        archive_sha256,
        archive_url: fixed_url(&archive.name),
        checksum_url: fixed_url(&checksum.name),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeInstallProgress {
    Resolving,
    Downloading { received: u64, total: u64 },
    Verifying,
    Extracting,
}

#[derive(Debug)]
pub struct RuntimeInstallError {
    pub message: String,
    pub retryable: bool,
}

impl RuntimeInstallError {
    fn http(context: &str, error: reqwest::Error) -> Self {
        let mut details = Vec::new();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            details.push(cause.to_string());
            source = cause.source();
        }
        let detail = if details.is_empty() {
            String::new()
        } else {
            format!("（{}）", details.join(" → "))
        };
        Self {
            retryable: error.is_connect()
                || error.is_timeout()
                || error.is_body()
                || error
                    .status()
                    .is_some_and(|status| status.is_server_error() || status.as_u16() == 429),
            message: format!("{context}：{error}{detail}"),
        }
    }
}

impl std::fmt::Display for RuntimeInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RuntimeInstallError {}

impl From<String> for RuntimeInstallError {
    fn from(message: String) -> Self {
        Self {
            message,
            retryable: false,
        }
    }
}

impl From<&str> for RuntimeInstallError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

/// A verified archive extracted to a private executable. Dropping preparation,
/// including cancellation, removes temporary files and releases the installer lock.
/// The previous runtime is untouched until activate atomically replaces it.
#[derive(Debug)]
pub struct PreparedRuntime {
    pub release: RuntimeRelease,
    binary: tempfile::TempPath,
    directory: PathBuf,
    _staging: tempfile::TempDir,
    _lock: fs::File,
}

impl PreparedRuntime {
    pub fn executable(&self) -> &Path {
        &self.binary
    }

    pub fn activate(self) -> Result<PathBuf, String> {
        let destination = self.directory.join(runtime_filename());
        self.binary
            .persist(&destination)
            .map_err(|error| format!("无法启用 rpi，原有版本已保留：{error}"))?;
        Ok(destination)
    }
}

/// Keep the existing settings entry on the same verified installation path.
pub async fn install_latest(mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    prepare_latest(|event| {
        if let RuntimeInstallProgress::Downloading { received, total } = event {
            progress(received, total);
        }
    })
    .await
    .map_err(|error| error.to_string())?
    .activate()
}

pub async fn prepare_latest(
    mut progress: impl FnMut(RuntimeInstallProgress),
) -> Result<PreparedRuntime, RuntimeInstallError> {
    let directory = install_dir()?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建 rpi 安装目录：{error}"))?;
    let lock = lock_install(&directory)?;
    progress(RuntimeInstallProgress::Resolving);
    let release = release_metadata().await?;
    prepare_archive(release, directory, lock, http_client()?, progress).await
}

fn lock_install(directory: &Path) -> Result<fs::File, RuntimeInstallError> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("install.lock"))
        .map_err(|error| error.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "另一个窗口正在安装 rpi，请稍后重试".to_string())?;
    Ok(lock)
}

async fn prepare_archive(
    release: RuntimeRelease,
    directory: PathBuf,
    lock: fs::File,
    client: reqwest::Client,
    mut progress: impl FnMut(RuntimeInstallProgress),
) -> Result<PreparedRuntime, RuntimeInstallError> {
    let staging = tempfile::Builder::new()
        .prefix(".rpi-")
        .tempdir_in(&directory)
        .map_err(|error| error.to_string())?;
    let archive = staging.path().join(&release.archive_name);
    progress(RuntimeInstallProgress::Downloading {
        received: 0,
        total: release.archive_size,
    });
    let mut response = client
        .get(&release.archive_url)
        .send()
        .await
        .map_err(|error| RuntimeInstallError::http("rpi 下载失败", error))?
        .error_for_status()
        .map_err(|error| RuntimeInstallError::http("rpi 下载失败", error))?;
    if response
        .content_length()
        .is_some_and(|size| size != release.archive_size)
    {
        return Err("rpi 下载大小与 GitHub 摘要不一致".into());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)
        .map_err(|error| error.to_string())?;
    let mut received = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| RuntimeInstallError::http("rpi 下载中断", error))?
    {
        received = received.saturating_add(chunk.len() as u64);
        if received > release.archive_size || received > MAX_ARCHIVE {
            return Err("rpi 下载超过大小限制".into());
        }
        output
            .write_all(&chunk)
            .map_err(|error| error.to_string())?;
        progress(RuntimeInstallProgress::Downloading {
            received,
            total: release.archive_size,
        });
    }
    output.sync_all().map_err(|error| error.to_string())?;
    drop(output);
    progress(RuntimeInstallProgress::Verifying);
    if received != release.archive_size || sha256(&archive)? != release.archive_sha256 {
        return Err("rpi 下载包 SHA-256 校验失败，已阻止安装".into());
    }
    let checksum = client
        .get(&release.checksum_url)
        .send()
        .await
        .map_err(|error| RuntimeInstallError::http("rpi 校验文件下载失败", error))?
        .error_for_status()
        .map_err(|error| RuntimeInstallError::http("rpi 校验文件下载失败", error))?;
    let checksum = read_bounded(checksum, MAX_CHECKSUM).await?;
    let checksum = std::str::from_utf8(&checksum)
        .map_err(|_| "rpi 校验文件编码无效")?
        .split_whitespace()
        .next()
        .ok_or("rpi 校验文件为空")?;
    if checksum.to_ascii_lowercase() != release.archive_sha256 {
        return Err("GitHub rpi 校验文件与发布摘要不一致".into());
    }
    progress(RuntimeInstallProgress::Extracting);
    let mut binary = tempfile::Builder::new()
        .prefix("rpi-")
        .suffix(std::env::consts::EXE_SUFFIX)
        .tempfile_in(staging.path())
        .map_err(|error| error.to_string())?;
    extract_binary(&archive, binary.as_file_mut(), &release)?;
    set_executable(binary.path())?;
    Ok(PreparedRuntime {
        release,
        binary: binary.into_temp_path(),
        directory,
        _staging: staging,
        _lock: lock,
    })
}

pub fn install_dir() -> Result<PathBuf, String> {
    let root = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
    };
    root.map(|root| root.join("rpi-work/runtime"))
        .ok_or("无法定位 rpi 安装目录".into())
}

fn runtime_filename() -> &'static str {
    if cfg!(windows) { "rpi.exe" } else { "rpi" }
}

async fn read_bounded(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, RuntimeInstallError> {
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| RuntimeInstallError::http("GitHub 响应中断", error))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("GitHub 响应超过大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn safe_archive_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn copy_runtime(mut reader: impl Read, output: &mut fs::File) -> Result<(), String> {
    let size = std::io::copy(&mut reader.by_ref().take(MAX_RUNTIME + 1), output)
        .map_err(|error| error.to_string())?;
    if size == 0 || size > MAX_RUNTIME {
        return Err("rpi 可执行文件大小无效".into());
    }
    output.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn extract_binary(
    archive: &Path,
    destination: &mut fs::File,
    release: &RuntimeRelease,
) -> Result<(), String> {
    let expected = runtime_filename();
    let mut found = false;
    if release.target.contains("windows") {
        let mut zip =
            zip::ZipArchive::new(fs::File::open(archive).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
            let path = entry
                .enclosed_name()
                .ok_or("rpi ZIP 包含越界路径")?
                .to_owned();
            if !safe_archive_path(&path) {
                return Err("rpi ZIP 包含不安全路径".into());
            }
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("rpi ZIP 包含符号链接".into());
            }
            if path.file_name().is_some_and(|name| name == expected) && !entry.is_dir() {
                if found {
                    return Err("rpi ZIP 包含多个可执行文件".into());
                }
                copy_runtime(&mut entry, destination)?;
                found = true;
            }
        }
    } else {
        let input = flate2::read::GzDecoder::new(
            fs::File::open(archive).map_err(|error| error.to_string())?,
        );
        let mut tar = tar::Archive::new(input);
        for entry in tar.entries().map_err(|error| error.to_string())? {
            let mut entry = entry.map_err(|error| error.to_string())?;
            let path = entry
                .path()
                .map_err(|error| error.to_string())?
                .into_owned();
            if !safe_archive_path(&path) {
                return Err("rpi TAR 包含不安全路径".into());
            }
            if entry.header().entry_type().is_symlink()
                || entry.header().entry_type().is_hard_link()
            {
                return Err("rpi TAR 包含链接".into());
            }
            if path.file_name().is_some_and(|name| name == expected) {
                if found || !entry.header().entry_type().is_file() {
                    return Err("rpi TAR 包含多个或无效的可执行文件".into());
                }
                copy_runtime(&mut entry, destination)?;
                found = true;
            }
        }
    }
    found
        .then_some(())
        .ok_or_else(|| "rpi 下载包中没有找到可执行文件".into())
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "rpi_runtime_tests.rs"]
mod tests;
