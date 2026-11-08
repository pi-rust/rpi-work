use crate::{MAX_DOWNLOAD, Release, http_client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub directory: PathBuf,
    pub version: String,
    pub android: bool,
}

pub fn cache_dir() -> Result<PathBuf, String> {
    let root = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|v| PathBuf::from(v).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".cache")))
    }
    .ok_or("无法定位本设备更新缓存")?;
    Ok(root.join("rpi-work/updates"))
}

pub fn sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn verify_download(path: &Path, release: &Release) -> Result<(), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() != release.artifact.size
        || sha256(path)? != release.artifact.sha256.to_lowercase()
    {
        return Err("更新文件大小或 SHA-256 不匹配，已阻止安装".into());
    }
    Ok(())
}

pub async fn download(
    release: &Release,
    root: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<Prepared, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = root.join(format!("{}-{nonce}", release.manifest.version));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let partial = directory.join("download.part");
    let result = async {
        let mut response = http_client()?
            .get(release.download_url())
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|size| size != release.artifact.size)
        {
            return Err("更新下载大小与签名清单不一致".into());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| e.to_string())?;
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            received += chunk.len() as u64;
            if received > release.artifact.size || received > MAX_DOWNLOAD {
                return Err("更新下载超过大小限制".into());
            }
            output.write_all(&chunk).map_err(|e| e.to_string())?;
            progress(received, release.artifact.size);
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        verify_download(&partial, release)?;
        if release.target.contains("android") {
            fs::rename(&partial, directory.join("update.apk")).map_err(|e| e.to_string())?;
        } else {
            extract(&partial, &directory, release)?;
            fs::remove_file(&partial).map_err(|e| e.to_string())?;
        }
        Ok(Prepared {
            directory: directory.clone(),
            version: release.manifest.version.clone(),
            android: release.target.contains("android"),
        })
    }
    .await;
    if result.is_err() {
        // Only this newly created directory is removed; existing downloads are untouched.
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn copy_limited(mut reader: impl Read, destination: &Path) -> Result<(), String> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let size = std::io::copy(&mut reader.by_ref().take(MAX_DOWNLOAD + 1), &mut output)
        .map_err(|e| e.to_string())?;
    if size == 0 || size > MAX_DOWNLOAD {
        return Err("解压后的更新文件大小无效".into());
    }
    output.sync_all().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            destination,
            fs::Permissions::from_mode(
                if destination
                    .file_name()
                    .is_some_and(|name| name == "Info.plist")
                {
                    0o644
                } else {
                    0o755
                },
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn extract(archive: &Path, directory: &Path, release: &Release) -> Result<(), String> {
    let prefix = format!("rpi-work-{}-{}", release.manifest.version, release.target);
    let windows = release.target.contains("windows");
    let ui = if windows { "work-ui.exe" } else { "work-ui" };
    let cli = if windows { "work-cli.exe" } else { "work-cli" };
    let ui_entry = if release.target.contains("apple") {
        format!("{prefix}/RPI Work.app/Contents/MacOS/work-ui")
    } else {
        format!("{prefix}/{ui}")
    };
    let cli_entry = format!("{prefix}/{cli}");
    let selected = |path: &Path| -> Result<Option<PathBuf>, String> {
        if !safe_path(path) {
            return Err("更新压缩包含不安全路径".into());
        }
        let path = path.to_string_lossy().replace('\\', "/");
        if path == ui_entry {
            Ok(Some(directory.join(ui)))
        } else if path == cli_entry {
            Ok(Some(directory.join(cli)))
        } else if release.target.contains("apple")
            && path == format!("{prefix}/RPI Work.app/Contents/Info.plist")
        {
            Ok(Some(directory.join("Info.plist")))
        } else {
            Ok(None)
        }
    };
    if windows {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
            let path = entry.enclosed_name().ok_or("更新ZIP包含越界路径")?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("更新ZIP包含符号链接".into());
            }
            if let Some(destination) = selected(&path)? {
                copy_limited(&mut entry, &destination)?;
            }
        }
    } else {
        let input =
            flate2::read::GzDecoder::new(fs::File::open(archive).map_err(|e| e.to_string())?);
        let mut tar = tar::Archive::new(input);
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            if entry.header().entry_type().is_symlink()
                || entry.header().entry_type().is_hard_link()
            {
                return Err("更新包包含链接".into());
            }
            let destination = selected(&entry.path().map_err(|e| e.to_string())?)?;
            if let Some(destination) = destination {
                if !entry.header().entry_type().is_file() {
                    return Err("更新程序不是普通文件".into());
                }
                copy_limited(&mut entry, &destination)?;
            }
        }
    }
    if !directory.join(ui).is_file()
        || !directory.join(cli).is_file()
        || (release.target.contains("apple") && !directory.join("Info.plist").is_file())
    {
        return Err("更新包缺少UI或CLI程序".into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct InstallPlan {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub ui_sha256: String,
    pub cli_sha256: String,
    #[serde(default)]
    pub info_sha256: Option<String>,
}

pub fn start_install(prepared: &Prepared) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let ui = executable.file_name().ok_or("无法定位当前程序")?;
    if ui
        != if cfg!(windows) {
            "work-ui.exe"
        } else {
            "work-ui"
        }
    {
        return Err("当前程序已改名，请重新解压原名安装包后再更新".into());
    }
    let cli = if cfg!(windows) {
        "work-cli.exe"
    } else {
        "work-cli"
    };
    let plan = InstallPlan {
        executable: executable.clone(),
        directory: prepared.directory.clone(),
        ui_sha256: sha256(&prepared.directory.join(ui))?,
        cli_sha256: sha256(&prepared.directory.join(cli))?,
        info_sha256: if cfg!(target_os = "macos") {
            Some(sha256(&prepared.directory.join("Info.plist"))?)
        } else {
            None
        },
    };
    let path = prepared.directory.join("install.json");
    fs::write(&path, serde_json::to_vec(&plan).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let helper = prepared.directory.join(if cfg!(windows) {
        "apply-update.exe"
    } else {
        "apply-update"
    });
    fs::copy(&executable, &helper).map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(helper);
    command.arg("--apply-update").arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Invoked by a copy of the currently trusted executable, before Dioxus starts.
pub fn run_helper(path: &Path) -> Result<(), String> {
    let plan: InstallPlan = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let result = apply(&plan);
    let log = plan.directory.join("install-result.txt");
    let _ = fs::write(
        &log,
        match &result {
            Ok(()) => "更新成功".to_owned(),
            Err(e) => format!("更新失败，已尝试回滚：{e}"),
        },
    );
    // A failed installation still reopens the old version when rollback succeeds.
    if plan.executable.is_file() {
        std::process::Command::new(&plan.executable)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    result
}

pub fn apply(plan: &InstallPlan) -> Result<(), String> {
    apply_with(plan, |source, destination| fs::copy(source, destination))
}

fn apply_with(
    plan: &InstallPlan,
    mut copy: impl FnMut(&Path, &Path) -> std::io::Result<u64>,
) -> Result<(), String> {
    let ui = if cfg!(windows) {
        "work-ui.exe"
    } else {
        "work-ui"
    };
    let cli = if cfg!(windows) {
        "work-cli.exe"
    } else {
        "work-cli"
    };
    if plan.executable.file_name().and_then(|v| v.to_str()) != Some(ui) {
        return Err("当前程序名称不支持原位更新，请重新解压安装".into());
    }
    if sha256(&plan.directory.join(ui))? != plan.ui_sha256
        || sha256(&plan.directory.join(cli))? != plan.cli_sha256
    {
        return Err("暂存更新文件已改变".into());
    }
    let ui_parent = plan.executable.parent().ok_or("程序路径无效")?;
    let mac_bundle = cfg!(target_os = "macos") && ui_parent.ends_with("Contents/MacOS");
    let cli_parent = if mac_bundle {
        ui_parent.ancestors().nth(3).ok_or("应用路径无效")?
    } else {
        ui_parent
    };
    let mut destinations = vec![plan.executable.clone(), cli_parent.join(cli)];
    let mut sources = vec![plan.directory.join(ui), plan.directory.join(cli)];
    if mac_bundle {
        let source = plan.directory.join("Info.plist");
        if plan.info_sha256.as_deref() != Some(sha256(&source)?.as_str()) {
            return Err("应用版本信息校验失败".into());
        }
        destinations.push(ui_parent.parent().ok_or("应用目录无效")?.join("Info.plist"));
        sources.push(source);
    }
    for destination in &destinations {
        if let Ok(metadata) = fs::symlink_metadata(destination)
            && !metadata.file_type().is_file()
        {
            return Err("安装目标不是普通文件，已阻止替换".into());
        }
    }
    let backups: Vec<_> = destinations
        .iter()
        .map(|p| p.with_extension("rpi-update-backup"))
        .collect();
    if backups.iter().any(|path| path.exists()) {
        return Err("发现上次更新的恢复文件，请先检查安装目录".into());
    }
    // Give the UI time to exit. Windows then releases its executable mapping.
    std::thread::sleep(Duration::from_millis(700));
    let mut moved = Vec::new();
    let mut touched = Vec::new();
    let outcome = (|| {
        for (index, destination) in destinations.iter().enumerate() {
            if destination.exists() {
                let mut last = None;
                for _ in 0..80 {
                    match fs::rename(destination, &backups[index]) {
                        Ok(()) => {
                            last = None;
                            break;
                        }
                        Err(error) => {
                            last = Some(error);
                            std::thread::sleep(Duration::from_millis(250));
                        }
                    }
                }
                if let Some(error) = last {
                    return Err(format!("安装目录不可写或程序仍被占用：{error}"));
                }
                moved.push(index);
            }
            touched.push(index);
            copy(&sources[index], destination).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if outcome.is_err() {
        for index in touched.into_iter().rev() {
            let _ = fs::remove_file(&destinations[index]);
        }
        for index in moved.into_iter().rev() {
            fs::rename(&backups[index], &destinations[index])
                .map_err(|e| format!("回滚失败：{e}；恢复文件：{}", backups[index].display()))?;
        }
    } else {
        for index in moved {
            let _ = fs::remove_file(&backups[index]);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_and_archive_paths_are_checked() {
        assert!(!safe_path(Path::new("../escape")));
        assert!(!safe_path(Path::new("/absolute")));
        assert!(safe_path(Path::new("package/work-ui")));
        let path = std::env::temp_dir().join(format!("rpi-update-hash-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn install_preserves_existing_files_when_staging_is_tampered() {
        let root = std::env::temp_dir().join(format!("rpi-update-rollback-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let ui = if cfg!(windows) {
            "work-ui.exe"
        } else {
            "work-ui"
        };
        fs::write(root.join(ui), b"existing").unwrap();
        let plan = InstallPlan {
            executable: root.join(ui),
            directory: root.join("missing"),
            ui_sha256: "wrong".into(),
            cli_sha256: "wrong".into(),
            info_sha256: None,
        };
        assert!(apply(&plan).is_err());
        assert_eq!(fs::read(root.join(ui)).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partial_install_rolls_back_and_success_replaces_both_programs() {
        let root =
            std::env::temp_dir().join(format!("rpi-update-transaction-{}", std::process::id()));
        let stage = root.join("staging");
        fs::create_dir_all(&stage).unwrap();
        let ui = if cfg!(windows) {
            "work-ui.exe"
        } else {
            "work-ui"
        };
        let cli = if cfg!(windows) {
            "work-cli.exe"
        } else {
            "work-cli"
        };
        for name in [ui, cli] {
            fs::write(root.join(name), b"old").unwrap();
            fs::write(stage.join(name), b"new").unwrap();
        }
        let plan = InstallPlan {
            executable: root.join(ui),
            directory: stage.clone(),
            ui_sha256: sha256(&stage.join(ui)).unwrap(),
            cli_sha256: sha256(&stage.join(cli)).unwrap(),
            info_sha256: None,
        };
        let mut count = 0;
        assert!(
            apply_with(&plan, |source, destination| {
                count += 1;
                if count == 2 {
                    Err(std::io::Error::other("simulated disk failure"))
                } else {
                    fs::copy(source, destination)
                }
            })
            .is_err()
        );
        for name in [ui, cli] {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"old");
        }
        apply(&plan).unwrap();
        for name in [ui, cli] {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"new");
            assert!(!root.join(name).with_extension("rpi-update-backup").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_requires_both_binaries_and_rejects_traversal() {
        use zip::write::SimpleFileOptions;
        let root = std::env::temp_dir().join(format!("rpi-update-archive-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("fixture.zip");
        let manifest = crate::Manifest {
            schema: 1,
            version: "0.2.0".into(),
            notes: String::new(),
            artifacts: Default::default(),
        };
        let release = Release {
            manifest,
            target: "x86_64-pc-windows-msvc".into(),
            artifact: crate::Artifact {
                name: "test.zip".into(),
                size: 1,
                sha256: "wrong".into(),
            },
        };
        for (case, entries) in [
            vec!["rpi-work-0.2.0-x86_64-pc-windows-msvc/work-ui.exe"],
            vec!["../escape"],
            vec![
                "rpi-work-0.2.0-x86_64-pc-windows-msvc/work-ui.exe",
                "rpi-work-0.2.0-x86_64-pc-windows-msvc/work-cli.exe",
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let output = root.join(case.to_string());
            fs::create_dir(&output).unwrap();
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            for name in entries {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(b"binary").unwrap();
            }
            zip.finish().unwrap();
            assert_eq!(extract(&archive, &output, &release).is_ok(), case == 2);
        }
        assert!(!root.join("escape").exists());
        assert!(verify_download(&archive, &release).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
