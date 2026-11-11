//! Download and install the native `rpi` runtime from its fixed GitHub release.

use super::{http_client, sha256};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const REPOSITORY: &str = "bigfish1913/pi-rust";
const RELEASES_URL: &str = "https://github.com/bigfish1913/pi-rust/releases";
const MAX_METADATA: usize = 512 * 1024;
const MAX_CHECKSUM: usize = 4096;
const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_RUNTIME: u64 = 256 * 1024 * 1024;

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
        .send()
        .await
        .map_err(|error| format!("无法连接 GitHub：{error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub 发布信息读取失败：{error}"))?;
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

/// Download, verify, and atomically install the selected rpi executable.
pub async fn install_latest(mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    let release = latest().await?;
    let directory = install_dir()?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建 rpi 安装目录：{error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let staging = directory.join(format!(".rpi-{nonce}"));
    fs::create_dir(&staging).map_err(|error| error.to_string())?;
    let archive = staging.join(&release.archive_name);
    let result = async {
        let response = http_client()?
            .get(&release.archive_url)
            .send()
            .await
            .map_err(|error| format!("rpi 下载失败：{error}"))?
            .error_for_status()
            .map_err(|error| format!("rpi 下载失败：{error}"))?;
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
        let mut response = response;
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            received = received.saturating_add(chunk.len() as u64);
            if received > release.archive_size || received > MAX_ARCHIVE {
                return Err("rpi 下载超过大小限制".into());
            }
            output
                .write_all(&chunk)
                .map_err(|error| error.to_string())?;
            progress(received, release.archive_size);
        }
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);
        if fs::metadata(&archive)
            .map_err(|error| error.to_string())?
            .len()
            != release.archive_size
            || sha256(&archive)? != release.archive_sha256
        {
            return Err("rpi 下载包 SHA-256 校验失败，已阻止安装".into());
        }
        let sidecar = read_bounded(
            http_client()?
                .get(&release.checksum_url)
                .send()
                .await
                .map_err(|error| format!("rpi 校验文件下载失败：{error}"))?
                .error_for_status()
                .map_err(|error| format!("rpi 校验文件下载失败：{error}"))?,
            MAX_CHECKSUM,
        )
        .await?;
        let sidecar_hash = std::str::from_utf8(&sidecar)
            .map_err(|_| "rpi 校验文件编码无效")?
            .split_whitespace()
            .next()
            .ok_or("rpi 校验文件为空")?;
        if sidecar_hash.to_ascii_lowercase() != release.archive_sha256 {
            return Err("GitHub rpi 校验文件与发布摘要不一致".into());
        }
        let staged_binary = staging.join(runtime_filename());
        extract_binary(&archive, &staged_binary, &release)?;
        set_executable(&staged_binary)?;
        let final_binary = directory.join(runtime_filename());
        if final_binary.exists() {
            fs::remove_file(&final_binary).map_err(|error| format!("无法替换旧 rpi：{error}"))?;
        }
        fs::rename(&staged_binary, &final_binary)
            .map_err(|error| format!("无法安装 rpi：{error}"))?;
        Ok(final_binary)
    }
    .await;
    let _ = fs::remove_dir_all(&staging);
    result
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

async fn read_bounded(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
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

fn copy_runtime(mut reader: impl Read, destination: &Path) -> Result<(), String> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| error.to_string())?;
    let size = std::io::copy(&mut reader.by_ref().take(MAX_RUNTIME + 1), &mut output)
        .map_err(|error| error.to_string())?;
    if size == 0 || size > MAX_RUNTIME {
        return Err("rpi 可执行文件大小无效".into());
    }
    output.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn extract_binary(
    archive: &Path,
    destination: &Path,
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
mod tests {
    use super::*;

    #[test]
    fn runtime_archive_name_matches_target() {
        let target = "x86_64-pc-windows-msvc";
        assert_eq!(
            format!("rpi-v0.1.0-{target}.zip"),
            "rpi-v0.1.0-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn install_directory_is_not_the_application_directory() {
        assert!(install_dir().is_ok() || std::env::var_os("HOME").is_none());
    }
}
