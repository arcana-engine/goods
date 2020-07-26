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
    alloc::{sync::Arc, vec::Vec},
    core::fmt::{self, Debug, Display},
    futures_core::future::{BoxFuture, LocalBoxFuture},
};

#[cfg(feature = "std")]
use std::error::Error;

#[cfg(not(feature = "std"))]
use core::fmt::Display as Error;

/// Error type for [`Source`]s.
///
/// [`Source`]: ./trait.Source.html
pub enum SourceError {
    /// Asset is not found in the source.
    NotFound,

    /// Custom source error.
    Error(Arc<dyn Error + Send + Sync>),
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
pub trait LocalSource<K: ?Sized>: Debug + Send + Sync + 'static {
    /// Reads asset asynchronously.
    /// Returns async bytes on success.
    /// Otherwise returns error `E` describing occurred problem.
    fn read(&self, key: &K) -> LocalBoxFuture<'_, Result<Vec<u8>, SourceError>>;
}

/// Asset data source.
pub trait Source<K: ?Sized>: Debug + Send + Sync + 'static {
    /// Reads asset asynchronously.
    /// Returns async bytes on success.
    /// Otherwise returns error `E` describing occurred problem.
    fn read(&self, key: &K) -> BoxFuture<'_, Result<Vec<u8>, SourceError>>;
}
