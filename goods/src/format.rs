use {
    crate::{
        asset::Asset,
        cache::{Cache, LocalCache},
    },
    std::{
        fmt::Debug,
        future::{ready, Future, Ready},
    },
};

/// Format trait interprets raw bytes as an asset.
/// It may also use context for asset instance creation
/// and [`Cache`] to load compound assets.
///
/// [`Cache`]: ./struct.Cache.html
pub trait Format<A, K>: Debug + 'static {
    /// Decoding future.
    type DecodeFuture: Future<Output = eyre::Result<A>> + Send + 'static;

    /// Decode asset intermediate representation from raw data using cache to fetch sub-assets.
    fn decode(self, key: K, bytes: Box<[u8]>, cache: &Cache<K>) -> Self::DecodeFuture;
}

/// Format trait interprets raw bytes as an asset.
/// It may also use context for asset instance creation
/// and [`LocalCache`] to load compound assets.
///
/// [`LocalCache`]: ./struct.LocalCache.html
pub trait LocalFormat<A, K>: Debug + 'static {
    /// Decoding future.
    type DecodeFuture: Future<Output = eyre::Result<A>> + 'static;

    /// Decode asset intermediate representation from raw data using cache to fetch sub-assets.
    fn decode_local(self, key: K, bytes: Box<[u8]>, cache: &LocalCache<K>) -> Self::DecodeFuture;
}

pub trait SimpleFormat<A, K>: Debug + 'static {
    fn decode_simple(self, key: K, bytes: Box<[u8]>) -> eyre::Result<A>;
}

impl<A, K, F> Format<A, K> for F
where
    A: Send + 'static,
    F: SimpleFormat<A, K> + 'static,
{
    type DecodeFuture = Ready<eyre::Result<A>>;

    fn decode(self, key: K, bytes: Box<[u8]>, _cache: &Cache<K>) -> Ready<eyre::Result<A>> {
        ready(self.decode_simple(key, bytes))
    }
}

impl<A, K, F> LocalFormat<A, K> for F
where
    A: 'static,
    F: SimpleFormat<A, K> + 'static,
{
    type DecodeFuture = Ready<eyre::Result<A>>;

    fn decode_local(
        self,
        key: K,
        bytes: Box<[u8]>,
        _cache: &LocalCache<K>,
    ) -> Ready<eyre::Result<A>> {
        ready(self.decode_simple(key, bytes))
    }
}

/// Default format for given asset type.
/// Allows calling [`Cache::load`] and make it use default format value for loading.
/// Has no effect otherwise.
///
/// [`Cache::load`]: /trait.Cache.html#tymethod.load
pub trait AssetDefaultFormat<K>: Asset {
    /// Format that will be used when asset is loaded using [`Cache::load`]
    ///
    /// [`Cache::load`]: /trait.Cache.html#tymethod.load
    type DefaultFormat: Default;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PassthroughFormat;

impl<A, K> SimpleFormat<A, K> for PassthroughFormat
where
    A: From<Box<[u8]>>,
{
    fn decode_simple(self, _: K, bytes: Box<[u8]>) -> eyre::Result<A> {
        Ok(bytes.into())
    }
}
