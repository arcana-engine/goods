use {
    crate::loader::Loader,
    std::{error::Error, future::Future, sync::Arc},
};

/// An asset type that can be built from decoded representation.
pub trait Asset: Clone + Sized + Send + Sync + 'static {
    /// Error building asset instance from decoded representation.
    type Error: Error + Send + Sync + 'static;

    /// Decoded representation of this asset.
    type Decoded: Send + Sync;

    /// Future that will resolve into decoded asset when ready.
    type Fut: Future<Output = Result<Self::Decoded, Self::Error>> + Send;

    /// Decode asset from bytes loaded from asset source.
    fn decode(bytes: Box<[u8]>, loader: &Loader) -> Self::Fut;
}

pub trait AssetBuild<B>: Asset {
    /// Build asset instance using decoded representation and `Resources`.
    fn build(decoded: Self::Decoded, builder: &mut B) -> Result<Self, Self::Error>;
}

#[doc(hidden)]
pub trait AssetContainer: Clone + Sized + Send + Sync + 'static {
    /// Error building asset instance from decoded representation.
    type Error: Error + Send + Sync + 'static;

    type Info: serde::de::DeserializeOwned;

    /// Decoded representation of this asset.
    type Decoded: Send + Sync;

    /// Future that will resolve into decoded asset when ready.
    type Fut: Future<Output = Result<Self::Decoded, Self::Error>> + Send;

    fn decode(info: Self::Info, loader: &Loader) -> Self::Fut;
}

#[doc(hidden)]
pub trait AssetContainerBuild<B>: AssetContainer {
    /// Build asset instance using decoded representation and `Resources`.
    fn build(decoded: Self::Decoded, builder: &mut B) -> Result<Self, Self::Error>;
}

impl<A> Asset for Arc<A>
where
    A: Asset,
{
    type Error = A::Error;
    type Decoded = A::Decoded;
    type Fut = A::Fut;

    fn decode(bytes: Box<[u8]>, loader: &Loader) -> Self::Fut {
        A::decode(bytes, loader)
    }
}

impl<A, B> AssetBuild<B> for Arc<A>
where
    A: AssetBuild<B>,
{
    fn build(decoded: Self::Decoded, builder: &mut B) -> Result<Self, Self::Error> {
        A::build(decoded, builder).map(Arc::new)
    }
}

impl<A> AssetContainer for Box<A>
where
    A: AssetContainer,
{
    type Info = A::Info;
    type Error = A::Error;
    type Decoded = A::Decoded;
    type Fut = A::Fut;

    fn decode(info: A::Info, loader: &Loader) -> Self::Fut {
        A::decode(info, loader)
    }
}

impl<A, B> AssetContainerBuild<B> for Box<A>
where
    A: AssetContainerBuild<B>,
{
    fn build(decoded: A::Decoded, builder: &mut B) -> Result<Self, Self::Error> {
        A::build(decoded, builder).map(Box::new)
    }
}

impl<A> AssetContainer for Arc<A>
where
    A: AssetContainer,
{
    type Info = A::Info;
    type Error = A::Error;
    type Decoded = A::Decoded;
    type Fut = A::Fut;

    fn decode(info: A::Info, loader: &Loader) -> Self::Fut {
        A::decode(info, loader)
    }
}

impl<A, B> AssetContainerBuild<B> for Arc<A>
where
    A: AssetContainerBuild<B>,
{
    fn build(decoded: A::Decoded, builder: &mut B) -> Result<Self, Self::Error> {
        A::build(decoded, builder).map(Arc::new)
    }
}

impl<A> AssetContainer for Arc<[A]>
where
    A: AssetContainer,
{
    type Info = Vec<A::Info>;
    type Error = A::Error;
    type Decoded = Vec<A::Decoded>;
    type Fut = futures::future::TryJoinAll<A::Fut>;

    fn decode(infos: Vec<A::Info>, loader: &Loader) -> Self::Fut {
        futures::future::try_join_all(infos.into_iter().map(|info| A::decode(info, loader)))
    }
}

impl<A, B> AssetContainerBuild<B> for Arc<[A]>
where
    A: AssetContainerBuild<B>,
{
    fn build(decoded: Vec<A::Decoded>, builder: &mut B) -> Result<Self, Self::Error> {
        decoded
            .into_iter()
            .map(|decoded| A::build(decoded, builder))
            .collect()
    }
}
