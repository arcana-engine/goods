//! Asset loading facility.

mod asset;
mod key;
mod loader;
pub mod source;

pub use {
    self::{
        asset::{Asset, AssetBuild, AssetContainer, AssetContainerBuild},
        loader::{AssetHandle, AssetResult, AssetResultPoisoned, Error, Loader, LoaderBuilder},
    },
    goods_proc::{Asset, AssetContainer},
    uuid::Uuid,
};

// Used by generated code.
#[doc(hidden)]
pub use {bincode, serde, serde_json, std::convert::Infallible, thiserror};

#[derive(Debug, thiserror::Error)]
#[error("Not found")]
struct NotFound;
