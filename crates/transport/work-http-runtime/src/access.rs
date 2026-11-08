//! Stable Work-side access facade for HTTP transports.
//!
//! This module is the replacement seam for the generic access/transport API that
//! will eventually be supplied by `rpi-httpcore`. It intentionally exposes only
//! transport lifecycle concepts and does not expose `LocalWorkspace` or Work
//! business protocol types. Until the upstream API is available, the concrete
//! implementation remains in this crate.

pub use crate::pairing::{Pairing, QrAuthorization};
pub use crate::security::{AuthLease, Security};
pub use crate::sharing::{Access, SharingControl};
pub use crate::{auth_router, AccessState};

/// The sharing epoch carried by an access snapshot and an authenticated lease.
///
/// Keep this alias at the adapter boundary so the upstream implementation can
/// replace the concrete integer without changing Work transport call sites.
pub type Generation = u64;
