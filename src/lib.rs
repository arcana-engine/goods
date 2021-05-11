//! Asset loading facility.

mod asset;
mod key;
mod loader;
mod source;

pub use self::{
    asset::Asset,
    loader::{AssetHandle, AssetResult, AssetResultPoisoned, Error, Loader, LoaderBuilder},
    source::{AssetData, Source},
};

#[derive(Debug, thiserror::Error)]
#[error("Not found")]
struct NotFound;
