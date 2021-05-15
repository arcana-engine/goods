use {
    crate::source::{AssetData, Source},
    std::{future::Future, path::Path, pin::Pin, sync::Arc},
    tokio::sync::Mutex,
    treasury::Treasury,
    uuid::Uuid,
};

pub use treasury::OpenError;

#[derive(Debug, thiserror::Error)]
#[error("Failed to access native file '{path}'")]
pub struct TreasuryFetchError {
    path: Box<Path>,
    source: std::io::Error,
}

pub struct TreasurySource {
    treasury: Arc<Mutex<Treasury>>,
}

impl TreasurySource {
    pub fn new(treasury: Treasury) -> Self {
        TreasurySource {
            treasury: Arc::new(Mutex::new(treasury)),
        }
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, OpenError> {
        let treasury = Treasury::open(root)?;
        Ok(TreasurySource::new(treasury))
    }
}

impl Source for TreasurySource {
    type Error = TreasuryFetchError;
    type Fut = Pin<Box<dyn Future<Output = Result<Option<AssetData>, TreasuryFetchError>> + Send>>;

    fn load(&self, uuid: &Uuid) -> Self::Fut {
        let treasury = self.treasury.clone();
        let uuid = *uuid;
        Box::pin(async move {
            let result = match treasury.lock().await.fetch(&uuid) {
                Ok(asset_data) => Ok(Some(AssetData {
                    bytes: asset_data.bytes,
                    version: asset_data.version,
                })),
                Err(treasury::FetchError::NotFound) => Ok(None),
                Err(treasury::FetchError::NativeIoError { source, path }) => {
                    Err(TreasuryFetchError { source, path })
                }
            };
            result
        })
    }

    fn update(&self, uuid: &Uuid, version: u64) -> Self::Fut {
        let treasury = self.treasury.clone();
        let uuid = *uuid;
        Box::pin(async move {
            let result = match treasury.lock().await.fetch_updated(&uuid, version) {
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
            result
        })
    }
}
