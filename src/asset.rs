use std::{
    convert::Infallible,
    future::{ready, Ready},
};

use {
    crate::loader::Loader,
    std::{error::Error, future::Future},
};

/// An asset type that can be built from decoded representation.
pub trait Asset: Clone + Sized + Send + Sync + 'static {
    /// Decoded representation of this asset.
    type Decoded: Send + Sync;

    /// Decoding error.
    type DecodeError: Error + Send + Sync + 'static;

    /// Building error.
    type BuildError: Error + Send + Sync + 'static;

    /// Future that will resolve into decoded asset when ready.
    type Fut: Future<Output = Result<Self::Decoded, Self::DecodeError>> + Send;

    /// Asset name.
    fn name() -> &'static str;

    /// Decode asset from bytes loaded from asset source.
    fn decode(bytes: Box<[u8]>, loader: &Loader) -> Self::Fut;
}

/// Asset building trait.
/// Users should implement this trait for at least single choice of `B`.
/// But it is highly recommended to implement this trait for wide choices of `B` for improved composability.
/// Because composite asset will implement `AssetBuild<B>` only for such types `B` for which all components implement `AssetBuild<B>`.
///
/// For example, if a reference of type `T` is necessary to build the asset,
/// implementing `AssetBuild<B> where B: Borrow<T>` (or `BorrowMut<T>` if mutable reference is needed)
/// would be recommended.
pub trait AssetBuild<B>: Asset {
    /// Build asset instance using decoded representation and `Resources`.
    fn build(decoded: Self::Decoded, builder: &mut B) -> Result<Self, Self::BuildError>;
}

/// Simple assets have no dependencies.
/// For this reason their `decode` function is always sync and do not take `Loader` argument.
pub trait SimpleAsset: Asset {
    /// Decode asset synchronously.
    fn decode(bytes: Box<[u8]>) -> Result<Self::Decoded, Self::DecodeError>;
}

/// Trivial assets have no dependencies and do not require building.
/// They are decoded directly from bytes.
/// And thus they implement `AssetBuild<B>` for any `B`.
pub trait TrivialAsset: Clone + Sized + Send + Sync + 'static {
    type Error: Error + Send + Sync + 'static;

    /// Asset name.
    fn name() -> &'static str;

    /// Decode asset directly.
    fn decode(bytes: Box<[u8]>) -> Result<Self, Self::Error>;
}

impl<A> Asset for A
where
    A: TrivialAsset,
{
    type Decoded = A;
    type DecodeError = A::Error;
    type BuildError = Infallible;
    type Fut = Ready<Result<A, A::Error>>;

    /// Asset name.
    fn name() -> &'static str {
        <A as TrivialAsset>::name()
    }

    fn decode(bytes: Box<[u8]>, _: &Loader) -> Ready<Result<A, A::Error>> {
        ready(<A as SimpleAsset>::decode(bytes))
    }
}

impl<A> SimpleAsset for A
where
    A: TrivialAsset,
{
    fn decode(bytes: Box<[u8]>) -> Result<A, A::Error> {
        TrivialAsset::decode(bytes)
    }
}

impl<A, B> AssetBuild<B> for A
where
    A: TrivialAsset,
{
    fn build(decoded: A, _: &mut B) -> Result<A, Infallible> {
        Ok(decoded)
    }
}
