#[cfg(feature = "treasury")]
pub mod treasury;

use std::error::Error;

use futures::future::BoxFuture;

use crate::AssetId;

/// Asset data loaded from [`Source`].
pub struct AssetData {
    /// Serialized asset data.
    pub bytes: Box<[u8]>,

    /// Opaque version for asset.
    /// It can only by interpreted by [`Source`]
    /// that returned this [`AssetData`] instance.
    pub version: u64,
}

/// Abstract source for asset raw data.
pub trait Source: Send + Sync + 'static {
    /// Error that may occur during asset loading.
    type Error: Error + Send + Sync;

    /// Searches for the asset by given path.
    /// Returns `Ok(Some(asset_data))` if asset is found and loaded successfully.
    /// Returns `Ok(None)` if asset is not found.
    fn find(&self, path: &str, asset: &str) -> BoxFuture<Option<AssetId>>;

    /// Load asset data from this source.
    /// Returns `Ok(Some(asset_data))` if asset is loaded successfully.
    /// Returns `Ok(None)` if asset is not found, allowing checking other sources.
    fn load(&self, id: AssetId) -> BoxFuture<Result<Option<AssetData>, Self::Error>>;

    /// Update asset data if newer is available.
    fn update(
        &self,
        id: AssetId,
        version: u64,
    ) -> BoxFuture<Result<Option<AssetData>, Self::Error>>;
}
