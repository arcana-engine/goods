use {
    crate::source::SourceError,
    alloc::sync::Arc,
    core::fmt::{self, Debug, Display},
};

#[cfg(feature = "std")]
use std::error::Error as StdError;

#[cfg(not(feature = "std"))]
use core::fmt::Display as StdError;

/// Error occured in process of asset loading.
pub enum Error {
    /// Asset was not found among registered sources.
    NotFound,

    /// Failed to spawn loading task.
    SpawnError,

    /// Asset instance building failed.
    ///
    /// Specifically this error may occur in [`Asset::build`].
    ///
    /// [`Asset::build`]: ./trait.Asset.html#tymethod.build
    Asset(Arc<dyn StdError + Send + Sync>),

    /// Asset decoding failed.
    ///
    /// Specifically this error may occur in [`Format::decode`].
    ///
    /// [`Format::decode`]: ./trait.Format.html#tymethod.decode
    Format(Arc<dyn StdError + Send + Sync>),

    /// Source in which asset was found failed to load it.
    Source(Arc<dyn StdError + Send + Sync>),
}

impl From<SourceError> for Error {
    fn from(err: SourceError) -> Self {
        match err {
            SourceError::NotFound => Error::NotFound,
            SourceError::Error(err) => Error::Source(err),
        }
    }
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::NotFound => Error::NotFound,
            Error::SpawnError => Error::SpawnError,
            Error::Asset(err) => Error::Asset(err.clone()),
            Error::Format(err) => Error::Format(err.clone()),
            Error::Source(err) => Error::Source(err.clone()),
        }
    }
}

impl Debug for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => fmt.write_str("Error::NotFound"),
            Error::SpawnError => fmt.write_str("Error::SpawnError"),
            Error::Asset(err) => write!(fmt, "Error::Asset({})", err),
            Error::Format(err) => write!(fmt, "Error::Format({})", err),
            Error::Source(err) => write!(fmt, "Error::Source({})", err),
        }
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => fmt.write_str("Asset not found"),
            Error::SpawnError => fmt.write_str("Failed to spawn loading task"),
            Error::Asset(err) => write!(fmt, "Asset error: {}", err),
            Error::Format(err) => write!(fmt, "Format error: {}", err),
            Error::Source(err) => write!(fmt, "Source error: {}", err),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NotFound => None,
            Error::SpawnError => None,
            Error::Asset(err) => Some(&**err),
            Error::Format(err) => Some(&**err),
            Error::Source(err) => Some(&**err),
        }
    }
}
