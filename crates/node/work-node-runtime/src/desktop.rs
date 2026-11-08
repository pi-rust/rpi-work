//! Native runtime resolution for the in-process workspace owner.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Explicit override, bundle sibling, then PATH. Never search unrelated workspaces.
#[must_use]
pub fn find_runtime(
    name: &str,
    override_path: Option<PathBuf>,
    application: &Path,
) -> Option<PathBuf> {
    find_runtime_on_path(name, override_path, application, std::env::var_os("PATH"))
}

/// The same lookup with an explicit PATH snapshot, for deterministic preparation.
pub(crate) fn find_runtime_on_path(
    name: &str,
    override_path: Option<PathBuf>,
    application: &Path,
    search_path: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return path.is_file().then_some(path);
    }
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::iter::once(application.join(&filename))
        .chain(
            std::env::split_paths(&search_path.unwrap_or_default())
                .map(|directory| directory.join(&filename)),
        )
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_is_preferred_and_missing_explicit_runtime_does_not_fall_back() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory
            .path()
            .join(format!("rpi{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&binary, "fixture").unwrap();
        assert_eq!(find_runtime("rpi", None, directory.path()), Some(binary));
        assert!(
            find_runtime(
                "rpi",
                Some(directory.path().join("missing")),
                directory.path()
            )
            .is_none()
        );
    }
}
