//! Native runtime resolution for the in-process workspace owner.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeSource {
    Override,
    Sibling,
    Path,
    Homebrew,
    Managed,
    CargoBin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeLookup {
    Found {
        path: PathBuf,
        source: RuntimeSource,
    },
    NoRuntime {
        searched: Vec<PathBuf>,
    },
    InvalidOverride {
        path: PathBuf,
    },
}

impl fmt::Display for RuntimeLookup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found { .. } => formatter.write_str("rpi 执行宿主已找到"),
            Self::NoRuntime { searched } => {
                write!(
                    formatter,
                    "未找到 rpi 执行宿主。请安装 rpi 或将其放在 RPI Work 可执行文件旁，然后重新启动。"
                )?;
                if !searched.is_empty() {
                    write!(formatter, "已检查：{}。", format_paths(searched))?;
                }
                Ok(())
            }
            Self::InvalidOverride { path } => write!(
                formatter,
                "RPI_WORK_RPI_BIN 指向的 rpi 程序不存在或不是文件：{}。请修正或清除该变量。",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RuntimeLookup {}

fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("、")
}

/// Resolve the native runtime without losing the reason a lookup failed.
/// An explicit override is authoritative and never falls back to another path.
#[must_use]
pub fn resolve_runtime(
    name: &str,
    override_path: Option<PathBuf>,
    application: &Path,
    search_path: Option<OsString>,
) -> RuntimeLookup {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = override_path {
        return if path.is_file() {
            RuntimeLookup::Found {
                path,
                source: RuntimeSource::Override,
            }
        } else {
            RuntimeLookup::InvalidOverride { path }
        };
    }

    let sibling = application.join(&filename);
    if sibling.is_file() {
        return RuntimeLookup::Found {
            path: sibling,
            source: RuntimeSource::Sibling,
        };
    }

    let mut searched = vec![sibling];
    for directory in std::env::split_paths(&search_path.unwrap_or_default()) {
        let candidate = directory.join(&filename);
        if candidate.is_file() {
            return RuntimeLookup::Found {
                path: candidate,
                source: RuntimeSource::Path,
            };
        }
        searched.push(candidate);
    }

    if let Some(path) = find_runtime_in_homebrew(name) {
        return RuntimeLookup::Found {
            path,
            source: RuntimeSource::Homebrew,
        };
    }
    if let Some(path) = find_runtime_in_work_bins(name) {
        return RuntimeLookup::Found {
            path,
            source: RuntimeSource::Managed,
        };
    }
    if let Some(path) = find_runtime_in_user_bins(name) {
        return RuntimeLookup::Found {
            path,
            source: RuntimeSource::CargoBin,
        };
    }

    searched.extend(homebrew_runtime_paths(name));
    searched.push(work_runtime_path(name));
    if let Some(path) = cargo_runtime_path(name) {
        searched.push(path);
    }
    RuntimeLookup::NoRuntime { searched }
}

/// Explicit override, bundle sibling, PATH, Homebrew, optional Work runtime,
/// then Cargo bin.
#[must_use]
pub fn find_runtime(
    name: &str,
    override_path: Option<PathBuf>,
    application: &Path,
) -> Option<PathBuf> {
    match resolve_runtime(name, override_path, application, std::env::var_os("PATH")) {
        RuntimeLookup::Found { path, .. } => Some(path),
        RuntimeLookup::NoRuntime { .. } | RuntimeLookup::InvalidOverride { .. } => None,
    }
}

fn homebrew_runtime_paths(name: &str) -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    ["/opt/homebrew/bin", "/usr/local/bin"]
        .into_iter()
        .map(|directory| PathBuf::from(directory).join(&filename))
        .collect()
}

fn find_runtime_in_homebrew(name: &str) -> Option<PathBuf> {
    homebrew_runtime_paths(name)
        .into_iter()
        .find(|path| path.is_file())
}

fn work_runtime_path(name: &str) -> PathBuf {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("rpi-work/runtime")
            .join(filename)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Library/Application Support/rpi-work/runtime")
            .join(filename)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_default()
            .join("rpi-work/runtime")
            .join(filename)
    }
}

fn cargo_runtime_path(name: &str) -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        })?;
    Some(
        cargo_home
            .join("bin")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
    )
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
#[cfg(test)]
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

    #[test]
    fn invalid_override_is_classified_without_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let sibling = directory
            .path()
            .join(format!("rpi{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&sibling, "fixture").unwrap();
        let missing = directory.path().join("missing-rpi");
        let result = resolve_runtime("rpi", Some(missing.clone()), directory.path(), None);
        assert_eq!(result, RuntimeLookup::InvalidOverride { path: missing });
    }

    #[test]
    fn homebrew_candidates_use_both_macos_prefixes() {
        let paths = homebrew_runtime_paths("rpi");
        if cfg!(target_os = "macos") {
            assert_eq!(paths.len(), 2);
            assert!(paths[0].ends_with("opt/homebrew/bin/rpi"));
            assert!(paths[1].ends_with("usr/local/bin/rpi"));
        } else {
            assert_eq!(paths, [] as [PathBuf; 0]);
        }
    }

    #[test]
    fn lookup_diagnostic_keeps_missing_runtime_actionable() {
        let message = RuntimeLookup::NoRuntime {
            searched: vec![PathBuf::from("/tmp/rpi")],
        }
        .to_string();
        assert!(message.starts_with("未找到 rpi 执行宿主"));
        assert!(message.contains("/tmp/rpi"));
    }
}
