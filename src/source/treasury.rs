use {
    crate::source::{AssetData, Source},
    std::{
        future::{ready, Ready},
        path::Path,
    },
    uuid::Uuid,
};

pub use treasury::Treasury;

#[derive(Debug, thiserror::Error)]
#[error("Failed to access native file '{path}'")]
pub struct TreasuryFetchError {
    path: Box<Path>,
    source: std::io::Error,
}

impl Source for Treasury {
    type Error = TreasuryFetchError;
    type Fut = Ready<Result<Option<AssetData>, TreasuryFetchError>>;

    fn load(&self, uuid: &Uuid) -> Self::Fut {
        let result = match self.fetch_frozen(uuid) {
            Ok(None) => Ok(None),
            Ok(Some(asset_data)) => Ok(Some(AssetData {
                bytes: asset_data.bytes,
                version: asset_data.version,
            })),
            Err(treasury::FetchError::NotFound) => Ok(None),
            Err(treasury::FetchError::NativeIoError { source, path }) => {
                Err(TreasuryFetchError { source, path })
            }
        };
        ready(result)
    }

    fn update(&self, _uuid: &Uuid, _version: u64) -> Self::Fut {
        ready(Ok(None))
    }
}
