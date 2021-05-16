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

    /// Decode asset from bytes loaded from asset source.
    fn decode(bytes: Box<[u8]>, loader: &Loader) -> Self::Fut;
}

pub trait AssetBuild<B>: Asset {
    /// Build asset instance using decoded representation and `Resources`.
    fn build(decoded: Self::Decoded, builder: &mut B) -> Result<Self, Self::BuildError>;
}
