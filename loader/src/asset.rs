use {
    crate::loader::Loader,
    std::{error::Error, future::Future},
};

/// An asset type that can be built from decoded representation.
pub trait Asset: Clone + Sized + Send + Sync + 'static {
    /// Error building asset instance from decoded representation.
    type Error: Error + Send + Sync + 'static;

    /// Decoded representation of this asset.
    type Decoded: Send + Sync;

    /// Builder required to build asset from decoded value.
    type Builder;

    /// Future that will resolve into decoded asset when ready.
    type Fut: Future<Output = Result<Self::Decoded, Self::Error>> + Send;

    /// Decode asset from bytes loaded from asset source.
    fn decode(bytes: Box<[u8]>, loader: Loader) -> Self::Fut;

    /// Build asset instance using decoded representation and `Resources`.
    fn build(decoded: Self::Decoded, builder: &mut Self::Builder) -> Result<Self, Self::Error>;
}