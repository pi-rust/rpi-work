//! Signed GPUI desktop updates and the restart-time installation transaction.

use super::{cache_dir, copy_limited, fetch_archive, safe_path, sha256, verify_download};
use crate::{
    GPUI_MANIFEST, Manifest, Release, has_asset, http_client, latest_metadata, signed_manifest,
};
use fs2::FileExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::OnceLock,
};

pub const APPLY_FLAG: &str = "--apply-gpui-update";
const HELPER_PREFIX: &str = "helper-";
const STAGE_PREFIX: &str = ".rpi-work-update-";
const PLAN: &str = "install.json";
const HELPER: &str = if cfg!(windows) {
    "apply-update.exe"
} else {
    "apply-update"
};
const EXECUTABLE: &str = if cfg!(windows) {
    "work-gpui.exe"
} else {
    "work-gpui"
};
pub const RESULT_ENV: &str = "RPI_WORK_GPUI_UPDATE_ERROR";
const CLEANUP_ENV: &str = "RPI_WORK_GPUI_UPDATE_CLEANUP";
const MAC_APP: &str = "RPI Work.app";
const MAC_BINARY: &str = "Contents/MacOS/work-gpui";
const MAC_FILES: [&str; 4] = [
    MAC_BINARY,
    "Contents/Info.plist",
    "Contents/Resources/rpi.icns",
    "Contents/_CodeSignature/CodeResources",
];

pub struct Prepared {
    download: tempfile::TempDir,
    release: Release,
}

#[derive(Serialize, Deserialize)]
struct InstallPlan {
    executable: PathBuf,
    previous_sha256: String,
    staged_sha256: Vec<String>,
    stage: PathBuf,
    arguments: Vec<String>,
}

fn expected_package(target: &str) -> Option<&'static str> {
    Some(match target {
        "x86_64-pc-windows-msvc" => "rpi-work-windows-x64.zip",
        "aarch64-pc-windows-msvc" => "rpi-work-windows-arm64.zip",
        "x86_64-apple-darwin" => "rpi-work-macos-x64.zip",
        "aarch64-apple-darwin" => "rpi-work-macos-arm64.zip",
        "x86_64-unknown-linux-gnu" => "rpi-work-linux-x64.tar.gz",
        "aarch64-unknown-linux-gnu" => "rpi-work-linux-arm64.tar.gz",
        _ => return None,
    })
}

fn select(manifest: Manifest, current: &str, target: &str) -> Result<Option<Release>, String> {
    if Version::parse(&manifest.version).map_err(|_| "更新版本无效")?
        <= Version::parse(current).map_err(|_| "当前版本无效")?
    {
        return Ok(None);
    }
    let expected = expected_package(target).ok_or("当前平台不支持自动更新")?;
    let artifact = manifest
        .artifacts
        .get(target)
        .cloned()
        .ok_or("签名清单缺少当前平台的更新包")?;
    if artifact.name != expected {
        return Err("更新包与当前平台不匹配".into());
    }
    Ok(Some(Release {
        manifest,
        target: target.into(),
        artifact,
    }))
}

pub async fn check(current: &str) -> Result<Option<Release>, String> {
    let client = http_client()?;
    let Some(metadata) = latest_metadata(&client).await? else {
        return Ok(None);
    };
    if !has_asset(&metadata, GPUI_MANIFEST)
        || metadata["draft"] == true
        || metadata["prerelease"] == true
    {
        return Ok(None);
    }
    let version = metadata["tag_name"]
        .as_str()
        .and_then(|tag| tag.strip_prefix('v'))
        .ok_or("发布标签无效")?;
    let parsed = Version::parse(version).map_err(|_| "发布版本号无效")?;
    if !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err("只支持稳定通道".into());
    }
    if parsed <= Version::parse(current).map_err(|_| "当前版本无效")? {
        return Ok(None);
    }
    let manifest = signed_manifest(&client, version, GPUI_MANIFEST).await?;
    select(manifest, current, crate::target())
}

pub async fn download(
    release: Release,
    progress: impl FnMut(u64, u64),
) -> Result<Prepared, String> {
    if release.target != crate::target()
        || expected_package(&release.target) != Some(release.artifact.name.as_str())
    {
        return Err("更新包与当前平台不匹配".into());
    }
    let root = update_cache()?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let download = tempfile::Builder::new()
        .prefix("download-")
        .tempdir_in(&root)
        .map_err(|error| error.to_string())?;
    fetch_archive(&release, &download.path().join("package"), progress).await?;
    Ok(Prepared { download, release })
}

pub fn supports_install() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| destination(&path).ok())
        .is_some()
}

fn update_cache() -> Result<PathBuf, String> {
    Ok(cache_dir()?.join("gpui"))
}

fn destination(executable: &Path) -> Result<PathBuf, String> {
    if executable.file_name() != Some(OsStr::new(EXECUTABLE)) || !executable.is_absolute() {
        return Err("请从原名发布包运行 RPI Work，以启用自动更新".into());
    }
    if cfg!(target_os = "macos") {
        executable
            .ancestors()
            .nth(3)
            .filter(|bundle| {
                bundle.extension() == Some(OsStr::new("app"))
                    && bundle.join(MAC_BINARY) == executable
            })
            .map(Path::to_owned)
            .ok_or_else(|| "请从已解压的 RPI Work.app 启动".into())
    } else {
        Ok(executable.to_owned())
    }
}

pub fn start_install(prepared: &Prepared, arguments: Vec<String>) -> Result<(), String> {
    // The helper waits for EOF. This handle stays open until the UI process exits,
    // including on platforms where the GUI event loop never returns from quit().
    static EXIT_PIPE: OnceLock<ChildStdin> = OnceLock::new();
    if EXIT_PIPE.get().is_some() {
        return Err("更新已安排，正在等待应用退出".into());
    }
    let root = update_cache()?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let destination = destination(&executable)?;
    let parent = destination.parent().ok_or("找不到安装目录")?;
    let stage = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(parent)
        .map_err(|error| format!("无法写入当前安装目录：{error}"))?;
    let package = prepared.download.path().join("package");
    verify_download(&package, &prepared.release)?;
    let staged = extract(&package, stage.path(), &prepared.release.target)?;
    if cfg!(target_os = "macos") {
        let codesign = Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&staged)
            .output()
            .map_err(|error| format!("无法验证 macOS 应用签名：{error}"))?;
        if !codesign.status.success() {
            return Err(format!(
                "macOS 应用签名校验失败：{}",
                String::from_utf8_lossy(&codesign.stderr).trim()
            ));
        }
    }
    let helper = tempfile::Builder::new()
        .prefix(HELPER_PREFIX)
        .tempdir_in(root)
        .map_err(|error| error.to_string())?;
    let plan = InstallPlan {
        previous_sha256: sha256(&executable)?,
        staged_sha256: staged_hashes(&staged)?,
        executable: executable.clone(),
        stage: stage.path().to_owned(),
        arguments,
    };
    let plan_path = helper.path().join(PLAN);
    fs::write(
        &plan_path,
        serde_json::to_vec(&plan).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let helper_path = helper.path().join(HELPER);
    fs::copy(&executable, &helper_path).map_err(|error| error.to_string())?;
    let mut command = Command::new(helper_path);
    command.arg(APPLY_FLAG).arg(plan_path).stdin(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动安装程序：{error}"))?;
    let exit_pipe = child.stdin.take().expect("installer stdin is piped");
    if EXIT_PIPE.set(exit_pipe).is_err() {
        let _ = child.kill();
        return Err("更新已安排，正在等待应用退出".into());
    }
    let _ = stage.keep();
    let _ = helper.keep();
    // The caller drops the download guard before quitting, even when the event loop exits directly.
    Ok(())
}

fn extract(archive: &Path, staging: &Path, target: &str) -> Result<PathBuf, String> {
    let mac = target.ends_with("apple-darwin");
    let linux = target.ends_with("unknown-linux-gnu");
    let source = fs::File::open(archive).map_err(|error| error.to_string())?;
    if linux {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(source));
        for entry in tar.entries().map_err(|error| error.to_string())? {
            let mut entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path().map_err(|error| error.to_string())?;
            if path.as_ref() != Path::new("work-gpui") || !entry.header().entry_type().is_file() {
                return Err("Linux 更新包包含非预期文件".into());
            }
            copy_limited(&mut entry, &staging.join("work-gpui"))?;
        }
    } else {
        let mut zip = zip::ZipArchive::new(source).map_err(|error| error.to_string())?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
            let path = entry
                .enclosed_name()
                .filter(|path| safe_path(path))
                .ok_or("更新包包含越界路径")?;
            if entry.is_symlink() {
                return Err("更新包包含符号链接".into());
            }
            if entry.is_dir() || mac && path.starts_with("__MACOSX") {
                continue;
            }
            let expected = if mac {
                path.strip_prefix(MAC_APP).ok().is_some_and(|relative| {
                    MAC_FILES.iter().any(|name| relative == Path::new(name))
                })
            } else {
                path == Path::new("work-gpui.exe")
            };
            if !expected {
                return Err("更新包包含非预期文件".into());
            }
            let file = staging.join(path);
            fs::create_dir_all(file.parent().ok_or("更新文件路径无效")?)
                .map_err(|error| error.to_string())?;
            copy_limited(&mut entry, &file)?;
        }
    }
    let staged = staging.join(if mac {
        MAC_APP
    } else if linux {
        "work-gpui"
    } else {
        "work-gpui.exe"
    });
    let complete = if mac {
        MAC_FILES.iter().all(|file| staged.join(file).is_file())
    } else {
        staged.is_file()
    };
    if !complete {
        return Err("更新包缺少应用文件".into());
    }
    Ok(staged)
}

fn staged_hashes(staged: &Path) -> Result<Vec<String>, String> {
    if cfg!(target_os = "macos") {
        MAC_FILES
            .iter()
            .map(|file| sha256(&staged.join(file)))
            .collect()
    } else {
        Ok(vec![sha256(staged)?])
    }
}

pub fn run_helper(plan_path: &Path) -> Result<(), String> {
    let helper = plan_path.parent().ok_or("安装计划路径无效")?;
    let running = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| error.to_string())?;
    if plan_path.file_name() != Some(OsStr::new(PLAN))
        || helper.parent() != Some(update_cache()?.as_path())
        || !has_prefix(helper, HELPER_PREFIX)
        || fs::canonicalize(helper.join(HELPER)).ok().as_ref() != Some(&running)
    {
        return Err("安装计划路径无效".into());
    }
    let plan: InstallPlan =
        serde_json::from_reader(fs::File::open(plan_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let destination = destination(&plan.executable)?;
    if plan.stage.parent() != destination.parent() || !has_prefix(&plan.stage, STAGE_PREFIX) {
        return Err("更新暂存路径无效".into());
    }
    let mut data = [0];
    if io::stdin()
        .read(&mut data)
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err("安装程序等待应用退出失败".into());
    }
    let outcome = (|| {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(update_cache()?.join("install.lock"))
            .map_err(|error| error.to_string())?;
        FileExt::lock_exclusive(&lock).map_err(|error| error.to_string())?;
        if sha256(&plan.executable)? != plan.previous_sha256 {
            return Ok(());
        }
        let source = plan.stage.join(if cfg!(target_os = "macos") {
            MAC_APP
        } else {
            EXECUTABLE
        });
        if staged_hashes(&source)? != plan.staged_sha256 {
            return Err("暂存更新文件已改变".into());
        }
        replace(&destination, &source, &plan.stage.join("previous"), || {
            launch(&plan, helper, None)
        })
    })();
    // A failed restore leaves the backup in the staging directory for recovery.
    if outcome.is_ok() || !plan.stage.join("previous").exists() {
        let _ = fs::remove_dir_all(&plan.stage);
    }
    let _ = fs::remove_file(plan_path);
    if let Err(error) = &outcome
        && plan.executable.is_file()
        && let Err(relaunch) = launch(&plan, helper, Some(error))
    {
        return Err(format!("{error}。{relaunch}"));
    }
    outcome
}

fn replace(
    destination: &Path,
    source: &Path,
    backup: &Path,
    launch: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    if metadata.is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err("安装目标不是普通应用文件".into());
    }
    fs::rename(destination, backup).map_err(|error| format!("无法移动现有应用：{error}"))?;
    let install = fs::rename(source, destination).map_err(|error| format!("无法替换应用：{error}"));
    let rollback = |error: String| {
        fs::rename(backup, destination).map_err(|restore| {
            format!(
                "{error}。恢复失败：{restore}（备份位于 {}）",
                backup.display()
            )
        })?;
        Err(error)
    };
    if let Err(error) = install {
        return rollback(error);
    }
    if let Err(error) = launch() {
        fs::rename(destination, source).map_err(|restore| {
            format!(
                "{error}。恢复失败：{restore}（备份位于 {}）",
                backup.display()
            )
        })?;
        return rollback(error);
    }
    Ok(())
}

fn launch(plan: &InstallPlan, helper: &Path, error: Option<&str>) -> Result<(), String> {
    let mut command = Command::new(&plan.executable);
    command
        .args(&plan.arguments)
        .env(CLEANUP_ENV, helper)
        .env_remove(RESULT_ENV)
        .stdin(Stdio::null());
    if let Some(error) = error {
        command.env(RESULT_ENV, error);
    }
    command
        .spawn()
        .map(drop)
        .map_err(|error| format!("无法重新启动 RPI Work：{error}"))
}

pub fn cleanup_previous_helper() {
    let Some(path) = std::env::var_os(CLEANUP_ENV).map(PathBuf::from) else {
        return;
    };
    if path.parent() != update_cache().ok().as_deref() || !has_prefix(&path, HELPER_PREFIX) {
        return;
    }
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        let _ = fs::remove_file(path.join(HELPER));
        let _ = fs::remove_dir(path);
    }
}

fn has_prefix(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Artifact;
    use std::io::Write;

    #[test]
    fn selects_only_newer_packages_for_the_current_target() {
        let target = "aarch64-apple-darwin";
        let mut manifest = Manifest {
            schema: 1,
            version: "1.2.3".into(),
            notes: String::new(),
            artifacts: [(
                target.into(),
                Artifact {
                    name: "rpi-work-macos-arm64.zip".into(),
                    size: 128,
                    sha256: "a".repeat(64),
                },
            )]
            .into(),
        };
        assert!(select(manifest.clone(), "1.2.3", target).unwrap().is_none());
        assert!(select(manifest.clone(), "2.0.0", target).unwrap().is_none());
        assert!(select(manifest.clone(), "1.2.2", target).unwrap().is_some());
        assert!(select(manifest.clone(), "1.2.2", "x86_64-apple-darwin").is_err());
        manifest.artifacts.get_mut(target).unwrap().name = "rpi-work-macos-x64.zip".into();
        assert!(select(manifest, "1.2.2", target).is_err());
    }

    fn zip_fixture(path: &Path, files: &[&str]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for file in files {
            zip.start_file(*file, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"fixture").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_all_three_release_layouts_and_rejects_incomplete_bundles() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("fixture");
        let windows = root.path().join("windows");
        fs::create_dir(&windows).unwrap();
        zip_fixture(&archive, &["work-gpui.exe"]);
        assert_eq!(
            extract(&archive, &windows, "aarch64-pc-windows-msvc").unwrap(),
            windows.join("work-gpui.exe")
        );

        let files: Vec<_> = MAC_FILES
            .iter()
            .map(|file| format!("{MAC_APP}/{file}"))
            .collect();
        let mut files: Vec<_> = files.iter().map(String::as_str).collect();
        files.push("__MACOSX/._RPI Work.app");
        let mac = root.path().join("mac");
        zip_fixture(&archive, &files);
        assert_eq!(
            extract(&archive, &mac, "aarch64-apple-darwin").unwrap(),
            mac.join(MAC_APP)
        );
        zip_fixture(&archive, &files[..2]);
        assert!(
            extract(
                &archive,
                &root.path().join("incomplete"),
                "aarch64-apple-darwin"
            )
            .is_err()
        );
        zip_fixture(&archive, &["../work-gpui.exe"]);
        assert!(extract(&archive, &windows, "x86_64-pc-windows-msvc").is_err());

        let bytes = b"fixture";
        let gzip = flate2::write::GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(bytes.len() as u64);
        tar.append_data(&mut header, "work-gpui", bytes.as_slice())
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let linux = root.path().join("linux");
        fs::create_dir(&linux).unwrap();
        assert_eq!(
            extract(&archive, &linux, "x86_64-unknown-linux-gnu").unwrap(),
            linux.join("work-gpui")
        );
    }

    #[test]
    fn rollback_keeps_the_existing_application_until_replacement_can_launch() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let staged = root.path().join("staged");
        let backup = root.path().join("backup");
        fs::write(&original, b"old").unwrap();
        assert!(replace(&original, &staged, &backup, || unreachable!()).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"old");
        fs::write(&staged, b"new").unwrap();
        assert!(replace(&original, &staged, &backup, || Err("cannot launch".into())).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"old");
        assert!(!backup.exists());
        replace(&original, &staged, &backup, || {
            assert_eq!(fs::read(&original).unwrap(), b"new");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"old");
    }
}
