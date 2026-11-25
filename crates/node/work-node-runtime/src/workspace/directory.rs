//! Opt-in directory pages for the workspace owner. The legacy full-listing
//! endpoint still delegates to extcore.
use std::{
    fs::{self, DirEntry},
    path::{Component, Path, PathBuf},
};

use rpi_extcore::reply::ReplyKind;
use sha2::{Digest, Sha256};
use work_node_api::api::{FileEntry, FileListing, FilePagination};

use super::{LocalError, LocalWorkspace, WorkspaceError};

const PAGE_SIZE: usize = 200;

impl LocalWorkspace {
    pub async fn files_page(
        &self,
        project_id: &str,
        path: &str,
        cursor: Option<&str>,
    ) -> Result<FileListing, WorkspaceError> {
        let start = cursor.map(parse_cursor).transpose()?;
        let root = self.project_root(project_id)?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || read_page(&root, &path, start.as_ref()))
            .await
            .map_err(|_| failure(ReplyKind::Internal, "directory worker failed"))?
    }
}

fn failure(kind: ReplyKind, message: &str) -> WorkspaceError {
    LocalError::Service {
        kind,
        message: message.into(),
    }
    .into()
}

fn parse_cursor(cursor: &str) -> Result<(String, usize), WorkspaceError> {
    let invalid = || failure(ReplyKind::InvalidInput, "invalid directory cursor");
    let (revision, offset) = cursor
        .strip_prefix("v1.")
        .and_then(|token| token.split_once('.'))
        .ok_or_else(invalid)?;
    if revision.len() != 64 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let offset = offset.parse::<usize>().map_err(|_| invalid())?;
    if offset == 0 || offset % PAGE_SIZE != 0 {
        return Err(invalid());
    }
    Ok((revision.into(), offset))
}

fn hidden(name: &str) -> bool {
    // Match extcore's project browsing policy for both input and resolved paths.
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        ".git" | ".rpi" | "node_modules" | "target" | ".deploy"
    ) || name.starts_with(".env")
}

fn browsable(path: &Path) -> bool {
    path.components().all(|component| {
        matches!(component, Component::Normal(name) if name.to_str().is_some_and(|name| !hidden(name)))
    })
}

fn resolve(root: &Path, path: &str) -> Result<(PathBuf, PathBuf), WorkspaceError> {
    if path.contains(['\\', '\0', ':']) {
        return Err(failure(ReplyKind::InvalidInput, "invalid project path"));
    }
    let relative = Path::new(path);
    let denied = || {
        failure(
            ReplyKind::AccessDenied,
            "path is outside the browsable project",
        )
    };
    if !browsable(relative) {
        return Err(denied());
    }
    let missing = || failure(ReplyKind::NotFound, "directory not found");
    let root = fs::canonicalize(root).map_err(|_| missing())?;
    let directory = fs::canonicalize(root.join(relative)).map_err(|_| missing())?;
    if !directory.strip_prefix(&root).is_ok_and(browsable) {
        return Err(denied());
    }
    Ok((root, directory))
}

struct Child {
    name: String,
    is_directory: bool,
    entry: DirEntry,
}

fn children(directory: &Path) -> Result<Vec<Child>, WorkspaceError> {
    let denied = || failure(ReplyKind::AccessDenied, "cannot read directory");
    let mut children = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound
            || error.kind() == std::io::ErrorKind::NotADirectory
        {
            failure(ReplyKind::NotFound, "directory not found")
        } else {
            denied()
        }
    })? {
        let entry = entry.map_err(|_| denied())?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if hidden(&name) || name.contains(['\\', ':']) {
            continue;
        }
        let kind = entry.file_type().map_err(|_| denied())?;
        if kind.is_dir() || kind.is_file() {
            children.push(Child {
                name,
                is_directory: kind.is_dir(),
                entry,
            });
        }
    }
    children.sort_by_cached_key(|child| {
        (
            !child.is_directory,
            child.name.to_lowercase(),
            child.name.clone(),
        )
    });
    Ok(children)
}

fn listing_revision(root: &Path, directory: &Path, path: &str, children: &[Child]) -> String {
    let mut digest = Sha256::new();
    let mut field = |bytes: &[u8]| {
        digest.update(bytes.len().to_le_bytes());
        digest.update(bytes);
    };
    field(root.as_os_str().as_encoded_bytes());
    field(directory.as_os_str().as_encoded_bytes());
    field(path.as_bytes());
    for child in children {
        field(&[u8::from(child.is_directory)]);
        field(child.name.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn read_page(
    root: &Path,
    path: &str,
    cursor: Option<&(String, usize)>,
) -> Result<FileListing, WorkspaceError> {
    let (root, directory) = resolve(root, path)?;
    let children = children(&directory)?;
    let revision = listing_revision(&root, &directory, path, &children);
    let start = if let Some((expected, offset)) = cursor {
        if *expected != revision || *offset >= children.len() {
            return Err(failure(ReplyKind::Conflict, "目录内容已变化，请刷新目录"));
        }
        *offset
    } else {
        0
    };
    let end = (start + PAGE_SIZE).min(children.len());
    let complete = end == children.len();
    let entries = children
        .into_iter()
        .skip(start)
        .take(end - start)
        .map(|child| {
            let size = child
                .entry
                .metadata()
                .ok()
                .filter(|metadata| {
                    if child.is_directory {
                        metadata.is_dir()
                    } else {
                        metadata.is_file()
                    }
                })
                .map_or(0, |metadata| metadata.len());
            FileEntry {
                path: if path.is_empty() {
                    child.name.clone()
                } else {
                    format!("{path}/{}", child.name)
                },
                kind: if child.is_directory {
                    "directory"
                } else {
                    "file"
                }
                .into(),
                name: child.name,
                size,
            }
        })
        .collect();
    Ok(FileListing {
        path: path.into(),
        entries,
        pagination: Some(FilePagination {
            complete,
            next_cursor: (!complete).then(|| format!("v1.{revision}.{end}")),
        }),
    })
}
