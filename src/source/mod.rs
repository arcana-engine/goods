#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
mod fs;

#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub use self::fs::*;

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
mod reqwest;

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
pub use self::reqwest::*;

use {
    alloc::{sync::Arc, vec::Vec},
    core::fmt::{self, Debug, Display},
    futures_core::future::BoxFuture,
};

/// Error type for `Source`s.
pub enum SourceError {
    /// File not found in the source.
    NotFound,

    /// Custom source error.
    #[cfg(feature = "std")]
    Error(Arc<dyn std::error::Error + Send + Sync>),

    /// Custom source error.
    #[cfg(not(feature = "std"))]
    Error(Arc<dyn core::fmt::Display + Send + Sync>),
}

impl Debug for SourceError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NotFound => fmt.write_str("SourceError::NotFound"),
            SourceError::Error(err) => write!(fmt, "SourceError::Error({})", err),
        }
    }
}

impl Display for SourceError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NotFound => fmt.write_str("Asset not found"),
            SourceError::Error(err) => write!(fmt, "Source error: {}", err),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SourceError::NotFound => None,
            SourceError::Error(err) => Some(&**err),
        }
    }
}

/// Asset data source.
pub trait Source<K: ?Sized>: Send + Sync + 'static {
    /// Reads asset asynchronously.
    /// Returns async bytes on success.
    /// Otherwise returns error `E` describing occurred problem.
    fn read(&self, key: &K) -> BoxFuture<'_, Result<Vec<u8>, SourceError>>;
}
