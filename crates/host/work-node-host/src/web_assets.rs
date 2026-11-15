//! Content-addressed Web assets also survive updates performed by older helpers.
use std::path::{Path, PathBuf};
include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub(super) fn prepare(data: &Path) -> Result<Option<PathBuf>, String> {
    if FILES.is_empty() {
        return Ok(None);
    }
    let destination = data.join(format!("work-web-{ID}"));
    if !destination.exists() {
        let staging = tempfile::Builder::new()
            .prefix(".work-web-")
            .tempdir_in(data)
            .map_err(|error| format!("无法准备内置 Web 目录。{error}"))?;
        for (relative, bytes) in FILES {
            let file = staging.path().join(relative);
            std::fs::create_dir_all(file.parent().ok_or("Web 资源目录无效")?)
                .and_then(|()| std::fs::write(file, bytes))
                .map_err(|error| format!("无法写入内置 Web 资源。{error}"))?;
        }
        std::fs::rename(staging.path(), &destination)
            .map_err(|error| format!("无法发布内置 Web 资源。{error}"))?;
    }
    for (relative, expected) in FILES {
        let actual = std::fs::read(destination.join(relative))
            .map_err(|_| "内置 Web 资源不完整，请重新准备节点目录")?;
        if actual != *expected {
            return Err("内置 Web 资源校验失败，请检查节点目录".into());
        }
    }
    Ok(Some(destination))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_web_prepares_once_and_rejects_changed_cache_bytes() {
        let data = tempfile::tempdir().unwrap();
        let prepared = prepare(data.path()).unwrap();
        if FILES.is_empty() {
            assert!(prepared.is_none());
            return;
        }
        let directory = prepared.unwrap();
        assert_eq!(prepare(data.path()).unwrap().as_ref(), Some(&directory));
        assert!(directory.join("index.html").is_file());
        let (relative, _) = FILES.first().unwrap();
        std::fs::write(directory.join(relative), b"changed Web cache").unwrap();
        assert!(prepare(data.path()).is_err());
    }
}
