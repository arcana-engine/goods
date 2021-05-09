use {
    goods::Goods,
    goods_loader::{AssetData, Source},
    std::{
        error::Error,
        future::{ready, Ready},
    },
    uuid::Uuid,
};

#[derive(Debug, thiserror::Error)]
pub enum GoodsError {
    #[error("Importer not found for the asset")]
    ImporterNotFound,

    #[error("Import failed")]
    ImporterError(#[source] Box<dyn Error + Send + Sync>),

    #[error("Failed to access source file")]
    SourceIoError(#[source] std::io::Error),

    #[error("Failed to access native file")]
    NativeIoError(#[source] std::io::Error),
}

pub struct GoodsSource(Goods);

impl Source for GoodsSource {
    type Error = GoodsError;
    type Fut = Ready<Result<Option<AssetData>, GoodsError>>;

    fn load(&self, uuid: &Uuid) -> Self::Fut {
        let result = match self.0.fetch_frozen(uuid) {
            Ok(None) => Ok(None),
            Ok(Some(asset_data)) => Ok(Some(AssetData {
                bytes: asset_data.bytes,
                version: asset_data.version,
            })),
            Err(goods::FetchError::NotFound) => Ok(None),
            Err(goods::FetchError::ImporterNotFound) => Err(GoodsError::ImporterNotFound),
            Err(goods::FetchError::ImporterError(err)) => Err(GoodsError::ImporterError(err)),
            Err(goods::FetchError::SourceIoError(err)) => Err(GoodsError::SourceIoError(err)),
            Err(goods::FetchError::NativeIoError(err)) => Err(GoodsError::NativeIoError(err)),
        };
        ready(result)
    }

    fn update(&self, _uuid: &Uuid, _version: u64) -> Self::Fut {
        ready(Ok(None))
    }
}
