//! Asset loading facility.

mod asset;
mod field;
mod key;
mod loader;
pub mod source;

pub use {
    self::{
        asset::{Asset, AssetBuild},
        field::{AssetField, AssetFieldBuild, Container, External},
        loader::{AssetHandle, AssetResult, AssetResultPoisoned, Error, Loader, LoaderBuilder},
    },
    goods_proc::{Asset, AssetField},
    uuid::Uuid,
};

// Used by generated code.
#[doc(hidden)]
pub use {bincode, serde, serde_json, std::convert::Infallible, thiserror};

#[derive(Debug, thiserror::Error)]
#[error("Not found")]
struct NotFound;
