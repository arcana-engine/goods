#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
mod fs;

#[cfg(all(feature = "fs", not(target_arch = "wasm32")))]
pub use self::fs::*;

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
mod reqwest;

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
pub use self::reqwest::*;

#[cfg(all(feature = "fetch", target_arch = "wasm32"))]
mod fetch;

#[cfg(all(feature = "fetch", target_arch = "wasm32"))]
pub use self::fetch::*;

use {
    crate::sync::{BoxFuture, Ptr, Send, Sync},
    alloc::vec::Vec,
    core::fmt::{self, Debug, Display},
};

/// Error type for `Source`s.
pub enum SourceError {
    /// File not found in the source.
    NotFound,

    /// Custom source error.
    #[cfg(all(not(feature = "std"), not(feature = "sync")))]
    Error(Ptr<dyn Display>),

    /// Custom source error.
    #[cfg(all(not(feature = "std"), not(not(feature = "sync"))))]
    Error(Ptr<dyn Display + Send + Sync>),

    /// Custom source error.
    #[cfg(all(feature = "std", not(feature = "sync")))]
    Error(Ptr<dyn std::error::Error>),

    /// Custom source error.
    #[cfg(all(feature = "std", feature = "sync"))]
    Error(Ptr<dyn std::error::Error + Send + Sync>),
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
