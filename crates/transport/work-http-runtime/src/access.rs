//! Stable Work-side access facade for HTTP transports.
//!
//! Work-specific authentication, sharing and pairing remain in this crate, while
//! generic bounded lifecycle primitives are re-exported from `rpi-httpcore`.
//! The facade intentionally exposes no `LocalWorkspace` or Work business
//! protocol types, so the owner and protocol adapters remain Work-owned.

pub use crate::pairing::{Pairing, QrAuthorization};
pub use crate::security::{AuthLease, Security};
pub use crate::sharing::{Access, SharingControl};
pub use crate::{auth_router, AccessState};

/// Generic bounded owner-operation admission from the shared HTTP core.
/// Work keeps its owner/executor adapter, but uses the package lifecycle
/// primitive rather than a transport-local semaphore.
pub use rpi_httpcore::access::{Admission, AdmissionError, AdmissionPermit};

/// The sharing epoch carried by an access snapshot and an authenticated lease.
///
/// Keep this alias at the adapter boundary so the upstream implementation can
/// replace the concrete integer without changing Work transport call sites.
pub type Generation = u64;
