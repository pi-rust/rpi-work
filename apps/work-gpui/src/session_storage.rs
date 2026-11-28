//! Device-local records scoped to a verified node and stable workspace identifiers.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::data::Snapshot;

const MAX_BYTES: u64 = 1024 * 1024;

pub(super) fn node_scope(snapshot: &Snapshot) -> Option<String> {
    if snapshot.node == "rpi-work://local" {
        Some("local".into())
    } else {
        let id = snapshot.node_id.as_ref().filter(|id| !id.is_empty())?;
        Some(format!("node:{id}"))
    }
}

pub(super) fn directory(name: &str) -> Option<PathBuf> {
    Some(work_viewmodel::connection::native_path()?.with_file_name(name))
}

pub(super) fn file(directory: &Path, node: &str, project: &str, session: Option<&str>) -> PathBuf {
    let mut hash = Sha256::new();
    for part in [node, project].into_iter().chain(session) {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    directory.join(format!("{:x}.json", hash.finalize()))
}

pub(super) fn read(path: &Path) -> io::Result<Option<Value>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("session record exceeds storage limit"));
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub(super) fn write(path: &Path, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("session record exceeds storage limit"));
    }
    let parent = path.parent().ok_or(io::ErrorKind::NotFound)?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
