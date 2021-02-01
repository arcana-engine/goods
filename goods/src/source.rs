use {
    futures_core::future::{BoxFuture, LocalBoxFuture},
    std::{
        error::Error,
        fmt::{self, Debug, Display},
    },
};

#[derive(Clone, Copy, Debug)]
pub struct AssetNotFound;

impl Display for AssetNotFound {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Asset not found")
    }
}

impl Error for AssetNotFound {}

pub trait AutoLocalSource {}

/// Asset data source.
pub trait Source<K>: Send + Sync + Debug + 'static {
    /// Reads asset asynchronously.
    /// Returns async bytes on success.
    /// Otherwise returns error `E` describing occurred problem.
    fn read(&self, key: &K) -> BoxFuture<'_, eyre::Result<Box<[u8]>>>;
}

/// Asset data source.
pub trait LocalSource<K>: Debug + 'static {
    /// Reads asset asynchronously.
    /// Returns async bytes on success.
    /// Otherwise returns error `E` describing occurred problem.
    fn read_local(&self, key: &K) -> LocalBoxFuture<'_, eyre::Result<Box<[u8]>>>;
}

impl<K, S> LocalSource<K> for S
where
    S: Source<K> + AutoLocalSource + Send + Sync,
{
    fn read_local(&self, key: &K) -> LocalBoxFuture<'_, eyre::Result<Box<[u8]>>> {
        Source::read(self, key)
    }
}
