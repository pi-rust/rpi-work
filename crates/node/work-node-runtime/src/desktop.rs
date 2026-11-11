//! Native runtime resolution for the in-process workspace owner.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Explicit override, bundle sibling, PATH, optional Work runtime, then Cargo bin.
#[must_use]
pub fn find_runtime(
    name: &str,
    override_path: Option<PathBuf>,
    application: &Path,
) -> Option<PathBuf> {
    if override_path.is_some() {
        return find_runtime_on_path(name, override_path, application, std::env::var_os("PATH"));
    }
    find_runtime_on_path(name, None, application, std::env::var_os("PATH"))
        .or_else(|| find_runtime_in_work_bins(name))
        .or_else(|| find_runtime_in_user_bins(name))
}

/// Look in the per-user directory used by the optional GitHub runtime installer.
fn find_runtime_in_work_bins(name: &str) -> Option<PathBuf> {
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
    }?;
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let candidate = root.join("rpi-work/runtime").join(filename);
    candidate.is_file().then_some(candidate)
}

/// Look in the conventional per-user Cargo installation directory as a
/// fallback for GUI launches that inherit a stale PATH from a desktop
/// shortcut. The explicit override, application sibling, and PATH remain the
/// authoritative lookup order; the installed Work runtime is checked before Cargo bin.
fn find_runtime_in_user_bins(name: &str) -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        })?;
    find_runtime_in_cargo_home(name, &cargo_home)
}

fn find_runtime_in_cargo_home(name: &str, cargo_home: &Path) -> Option<PathBuf> {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let candidate = cargo_home.join("bin").join(filename);
    candidate.is_file().then_some(candidate)
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
    fn cargo_user_bin_is_a_fallback_for_gui_launches() {
        let directory = tempfile::tempdir().unwrap();
        let cargo_home = directory.path().join("cargo");
        let bin = cargo_home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let runtime = bin.join(format!("rpi{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&runtime, "fixture").unwrap();
        assert_eq!(
            find_runtime_in_cargo_home("rpi", &cargo_home),
            Some(runtime)
        );
    }

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
