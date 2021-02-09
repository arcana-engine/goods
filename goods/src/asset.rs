use std::future::{ready, Future, Ready};

/// Loaded, processed and prepared asset.
/// This trait specifies how asset instances can be built from intermediate values
/// that are produced by `Format` implemetations.
///
/// [`Format`]: ./trait.Format.html
pub trait Asset: Clone + Sized + 'static {
    /// Intermediate representation type for the asset.
    /// This representation is constructed by [`Format::decode`].
    ///
    /// [`Format::decode`]: ./trait.Format.html#tymethod.decode
    type Repr;

    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    /// Special context type [`PhantomContext`] can be specified if no context is required,
    /// reducing overhead and making asset building occur faster.
    ///
    /// [`PhantomContext`]: ./struct.PhantomContext.html
    type Context;

    /// Asynchronous result produced by asset building.
    type BuildFuture: Future<Output = eyre::Result<Self>> + 'static;

    /// Build asset instance from intermediate representation using provided context.
    fn build(repr: Self::Repr, ctx: &mut Self::Context) -> Self::BuildFuture;
}

/// Shortcut for implementing [`Asset`] when asset building is synchronous.
///
/// [`Asset`]: ./trait.Asset.html
pub trait SyncAsset: Clone + Sized + 'static {
    /// Asset processing context.
    /// Instance of context is required to convert asset intermediate representation into asset instance.
    type Context;

    /// Intermediate representation type for the asset.
    /// This representation is constructed by [`Format::decode`].
    ///
    /// [`Format::decode`]: /trait.Format.html#tymethod.decode
    type Repr;

    /// Build asset instance from intermediate representation using provided context.
    fn build(repr: Self::Repr, ctx: &mut Self::Context) -> eyre::Result<Self>;
}

impl<S> Asset for S
where
    S: SyncAsset,
{
    type Repr = S::Repr;
    type Context = S::Context;
    type BuildFuture = Ready<eyre::Result<Self>>;

    #[inline]
    fn build(repr: S::Repr, ctx: &mut S::Context) -> Ready<eyre::Result<Self>> {
        ready(S::build(repr, ctx))
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
pub trait SimpleAsset: Clone + Sized + 'static {}

impl<S> SyncAsset for S
where
    S: SimpleAsset,
{
    type Repr = Self;
    type Context = PhantomContext;

    #[inline]
    fn build(repr: Self, _ctx: &mut PhantomContext) -> eyre::Result<Self> {
        Ok(repr)
    }
}
