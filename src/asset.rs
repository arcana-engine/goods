use {
    crate::{Cache, Ready},
    alloc::{fmt::Debug, vec::Vec},
    core::{convert::Infallible, future::Future},
};

#[cfg(feature = "std")]
use std::error::Error;

#[cfg(not(feature = "std"))]
use core::fmt::Display as Error;

/// Loaded, processed and prepared asset.
/// This trait specifies how asset instances can be built from intermediate values
/// that are produced by `Format` implemetations.
///
/// [`Format`]: ./trait.Format.html
pub trait Asset: Send + Sync + Sized + 'static {
    /// Error that may occur during asset building.
    type Error: Error + Send + Sync + 'static;

    /// Intermediate representation type for the asset.
    /// This representation is constructed by [`Format::decode`].
    ///
    /// [`Format::decode`]: ./trait.Format.html#tymethod.decode
    type Repr: Send;

    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    /// Special context type [`PhantomContext`] can be specified if no context is required,
    /// reducing overhead and making asset building occur faster.
    ///
    /// [`PhantomContext`]: ./struct.PhantomContext.html
    type Context;

    /// Asynchronous result produced by asset building.
    type BuildFuture: Future<Output = Result<Self, Self::Error>> + Send + 'static;

    /// Build asset instance from intermediate representation using provided context.
    fn build(repr: Self::Repr, ctx: &mut Self::Context) -> Self::BuildFuture;
}

/// Format trait interprets raw bytes as an asset.
/// It may also use context for asset instance creation
/// and [`Cache`] to load compound assets.
///
/// [`Cache`]: ./struct.Cache.html
pub trait Format<A: Asset, K>: Send + Sync + Debug + 'static {
    /// Error that may occur during asset loading.
    type Error: Error + Send + Sync + 'static;

    /// Asynchronous result produced by the format loading.
    type DecodeFuture: Future<Output = Result<A::Repr, Self::Error>> + Send + 'static;

    /// Decode asset intermediate representation from raw data using cache to fetch sub-assets.
    fn decode(self, bytes: Vec<u8>, cache: &Cache<K>) -> Self::DecodeFuture;
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
    type DefaultFormat: Format<Self, K> + Default;
}

/// Shortcut for implementing [`Asset`] when asset building is synchronous.
///
/// [`Asset`]: ./trait.Asset.html
pub trait SyncAsset: Send + Sync + Sized + 'static {
    /// Error that may occur during asset building.
    type Error: Error + Send + Sync + 'static;

    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    type Context;

    /// Intermediate representation type for the asset.
    /// This representation is constructed by [`Format::decode`].
    ///
    /// [`Format::decode`]: /trait.Format.html#tymethod.decode
    type Repr: Send;

    /// Build asset instance from intermediate representation using provided context.
    fn build(repr: Self::Repr, ctx: &mut Self::Context) -> Result<Self, Self::Error>;
}

impl<S> Asset for S
where
    S: SyncAsset,
{
    type Error = S::Error;
    type Repr = S::Repr;
    type Context = S::Context;
    type BuildFuture = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn build(repr: S::Repr, ctx: &mut S::Context) -> Ready<Result<Self, Self::Error>> {
        Ready(Some(S::build(repr, ctx)))
    }
}

/// Dummy context for assets that doesn't require one.
/// Reduces overhead and allows producing asset faster
/// without waiting for next [`Cache::process`] call.
///
/// [`Cache::process`]: ./struct.Cache.html#method.process
pub struct PhantomContext;

/// Shortcut for implementing [`Asset`] when asset is produced directly by [`Format`]
/// and no building is required.
///
/// [`Asset`]: ./trait.Asset.html
/// [`Format`]: ./trait.Format.html
pub trait SimpleAsset: Send + Sync + Sized + 'static {}

impl<S> SyncAsset for S
where
    S: SimpleAsset,
{
    type Error = Infallible;
    type Repr = Self;
    type Context = PhantomContext;

    #[inline]
    fn build(repr: Self, _ctx: &mut PhantomContext) -> Result<Self, Self::Error> {
        Ok(repr)
    }
}
