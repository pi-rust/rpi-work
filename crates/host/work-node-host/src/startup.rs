//! Typed startup failures exposed to both headless diagnostics and the GUI.

use serde::Serialize;
use std::{fmt, path::PathBuf};
use work_node_runtime::{desktop::RuntimeLookup, local::PrepareError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupFailureKind {
    RuntimeMissing,
    RuntimeInvalidOverride,
    DataLocked,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupFailure {
    pub kind: StartupFailureKind,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub searched: Vec<PathBuf>,
    /// The GUI may offer an automatic runtime install for this failure.
    pub can_install: bool,
}

impl StartupFailure {
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: StartupFailureKind::Internal,
            message: message.into(),
            searched: Vec::new(),
            can_install: false,
        }
    }

    #[must_use]
    pub fn from_prepare(error: PrepareError) -> Self {
        let message = error.to_string();
        match error {
            PrepareError::Runtime(lookup) => match lookup {
                RuntimeLookup::NoRuntime { searched } => Self {
                    kind: StartupFailureKind::RuntimeMissing,
                    message,
                    searched,
                    can_install: true,
                },
                RuntimeLookup::InvalidOverride { .. } => Self {
                    kind: StartupFailureKind::RuntimeInvalidOverride,
                    message,
                    searched: Vec::new(),
                    can_install: false,
                },
                RuntimeLookup::Found { .. } => Self::internal(message),
            },
            PrepareError::Locked => Self {
                kind: StartupFailureKind::DataLocked,
                message,
                searched: Vec::new(),
                can_install: false,
            },
            PrepareError::Message(message) => Self::internal(message),
        }
    }
}

impl fmt::Display for StartupFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StartupFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_keeps_install_option_and_searched_paths() {
        let failure =
            StartupFailure::from_prepare(PrepareError::Runtime(RuntimeLookup::NoRuntime {
                searched: vec![PathBuf::from("/tmp/rpi")],
            }));
        assert_eq!(failure.kind, StartupFailureKind::RuntimeMissing);
        assert!(failure.can_install);
        assert_eq!(failure.searched, [PathBuf::from("/tmp/rpi")]);
        assert!(failure.message.contains("/tmp/rpi"));
    }

    #[test]
    fn invalid_override_requires_manual_repair() {
        let failure =
            StartupFailure::from_prepare(PrepareError::Runtime(RuntimeLookup::InvalidOverride {
                path: PathBuf::from("/tmp/missing-rpi"),
            }));
        assert_eq!(failure.kind, StartupFailureKind::RuntimeInvalidOverride);
        assert!(!failure.can_install);
        assert!(failure.message.contains("RPI_WORK_RPI_BIN"));
    }

    #[test]
    fn lock_failure_is_not_an_install_problem() {
        let failure = StartupFailure::from_prepare(PrepareError::Locked);
        assert_eq!(failure.kind, StartupFailureKind::DataLocked);
        assert!(!failure.can_install);
        assert!(failure.message.contains("宿主"));
    }

    #[test]
    fn serialization_omits_empty_diagnostics() {
        let failure = StartupFailure::internal("internal detail");
        let value = serde_json::to_value(failure).unwrap();
        assert_eq!(value["kind"], "internal");
        assert_eq!(value["canInstall"], false);
        assert!(value.get("searched").is_none());
    }
}
