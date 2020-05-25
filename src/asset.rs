use {
    crate::{
        sync::{Send, Sync},
        Cache, Ready,
    },
    alloc::vec::Vec,
    core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

#[cfg(feature = "std")]
use std::error::Error;

#[cfg(not(feature = "std"))]
use core::fmt::Display;

/// Loaded, processed and prepared asset.
/// This trait specifies how asset instances can be built from intermediate values
/// that are produced by `Format` implemetations.
pub trait Asset: Send + Sync + Sized + Clone + 'static {
    /// Error that may occur during asset loading.
    #[cfg(feature = "std")]
    type Error: Error + Send + Sync;

    /// Error that may occur during asset loading.
    #[cfg(not(feature = "std"))]
    type Error: Display + Send + Sync;

    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    type Context;

    /// Intermediate representation type for the asset.
    /// This representation is constructed by `Format::decode`.
    type Repr: Send;

    /// Asynchronous result produced by asset building.
    type BuildFuture: Future<Output = Result<Self, Self::Error>> + Send + 'static;

    /// Build asset instance from intermediate representation using provided context.
    fn build(repr: Self::Repr, ctx: &mut Self::Context) -> Self::BuildFuture;
}

/// Format trait interprets raw bytes as an asset.
/// It may also use context for asset instance creation
/// and `Cache` to load compound assets.
pub trait Format<A: Asset, K>: Send + 'static {
    /// Asynchronous result produced by the format loading.
    type DecodeFuture: Future<Output = Result<A::Repr, A::Error>> + Send + 'static;

    /// Decode asset intermediate representation from raw data using cache to fetch sub-assets.
    fn decode(self, bytes: Vec<u8>, cache: &Cache<K>) -> Self::DecodeFuture;
}

/// Default format for given asset type.
pub trait AssetDefaultFormat<K>: Asset {
    /// Default format for asset.
    type DefaultFormat: Format<Self, K> + Default;
}

pub trait SyncAsset: Send + Sync + Sized + Clone + 'static {
    /// Error that may occur during asset loading.
    #[cfg(feature = "std")]
    type Error: Error + Send + Sync;

    /// Error that may occur during asset loading.
    #[cfg(not(feature = "std"))]
    type Error: Display + Send + Sync;

    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    type Context;

    /// Intermediate representation type for the asset.
    /// This representation is constructed by `Format::decode`.
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
pub struct PhantomContext;

/// Simplified asset trait to reduce boilerplace when implementing simple assets.
pub trait SimpleAsset: Send + Sync + Sized + Clone + 'static {
    /// Error that may occur during asset loading.
    #[cfg(feature = "std")]
    type Error: Error + Send + Sync;

    /// Error that may occur during asset loading.
    #[cfg(not(feature = "std"))]
    type Error: Display + Send + Sync;
}

impl<S> SyncAsset for S
where
    S: SimpleAsset,
{
    type Error = S::Error;
    type Repr = Self;
    type Context = PhantomContext;

    #[inline]
    fn build(repr: Self, _ctx: &mut PhantomContext) -> Result<Self, Self::Error> {
        Ok(repr)
    }
}
