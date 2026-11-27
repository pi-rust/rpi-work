//! Use the maintained xattr implementation for GPUI's legacy tar dependency.
//! The path operations in both versions act on symlinks without following them.

pub use xattr_current::{
    FileExt, SUPPORTED_PLATFORM, UnsupportedPlatformError, XAttrs, get, list, remove, set,
};
