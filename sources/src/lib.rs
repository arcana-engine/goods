use {
    goods::Goods,
    goods_loader::{AssetData, Source},
    std::{
        future::{ready, Ready},
        path::Path,
    },
    uuid::Uuid,
};

#[derive(Debug, thiserror::Error)]
#[error("Failed to access native file '{path}'")]
pub struct GoodsFetchError {
    path: Box<Path>,
    source: std::io::Error,
}

pub struct GoodsSource(Goods);

impl Source for GoodsSource {
    type Error = GoodsFetchError;
    type Fut = Ready<Result<Option<AssetData>, GoodsFetchError>>;

    fn load(&self, uuid: &Uuid) -> Self::Fut {
        let result = match self.0.fetch_frozen(uuid) {
            Ok(None) => Ok(None),
            Ok(Some(asset_data)) => Ok(Some(AssetData {
                bytes: asset_data.bytes,
                version: asset_data.version,
            })),
            Err(goods::FetchError::NotFound) => Ok(None),
            Err(goods::FetchError::NativeIoError { source, path }) => {
                Err(GoodsFetchError { source, path })
            }
        };
        ready(result)
    }

    fn update(&self, _uuid: &Uuid, _version: u64) -> Self::Fut {
        ready(Ok(None))
    }
}
