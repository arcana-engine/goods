use eyre::Context;
use futures::future::BoxFuture;
use url::Url;

use crate::AssetId;

use {
    crate::source::{AssetData, Source},
    std::{path::Path, sync::Arc},
    tokio::sync::Mutex,
    treasury_client::Client,
};

#[derive(Debug, thiserror::Error)]
pub enum TreasuryError {
    #[error(transparent)]
    Treasury { source: eyre::Report },

    #[error("Invalid URL '{url}'")]
    UrlError { url: Url },

    #[error("Failed to parse URL '{url}'")]
    UrlParseError { url: Box<str> },

    #[error("URL scheme is unsupported '{url}'")]
    UnsupportedUrlScheme { url: Url },

    #[error("Failed to access native file '{path}'")]
    IoError {
        source: std::io::Error,
        path: Box<Path>,
    },

    #[error("Async task aborted")]
    JoinError { source: tokio::task::JoinError },
}

pub struct TreasurySource {
    base_url: Url,
    treasury: Arc<Mutex<Client>>,
}

impl TreasurySource {
    pub async fn open_local(base: &Path) -> eyre::Result<Self> {
        let base = dunce::canonicalize(base)
            .wrap_err_with(|| format!("Failed to canonicalize base path '{}'", base.display()))?;

        let base_url = Url::from_directory_path(&base)
            .map_err(|()| eyre::eyre!("Failed to convert treasury path into URL"))?;

        let treasury = Client::local(base, false).await?;
        Ok(TreasurySource {
            base_url,
            treasury: Arc::new(Mutex::new(treasury)),
        })
    }
}

impl Source for TreasurySource {
    type Error = TreasuryError;

    fn find(&self, key: &str, asset: &str) -> BoxFuture<Option<AssetId>> {
        match self.base_url.join(key) {
            Err(_) => {
                tracing::debug!("Key '{}' is not valid URL. It cannot be treasury key", key);
                Box::pin(async { None })
            }
            Ok(url) => {
                let treasury = self.treasury.clone();
                let asset: Box<str> = asset.into();
                Box::pin(async move {
                    match treasury.lock().await.find(&url, &asset).await {
                        Ok(None) => None,
                        Ok(Some((tid, _))) => {
                            let id = AssetId(tid.value());
                            Some(id)
                        }
                        Err(err) => {
                            tracing::error!("Failed to find '{}' in treasury. {:#}", url, err);
                            None
                        }
                    }
                })
            }
        }
    }

    fn load(&self, id: AssetId) -> BoxFuture<Result<Option<AssetData>, Self::Error>> {
        let tid = treasury_client::AssetId::from(id.0);

        let treasury = self.treasury.clone();
        Box::pin(async move {
            match treasury.lock().await.fetch(tid).await {
                Ok(None) => Ok(None),
                Ok(Some(url)) => asset_data_from_url(url).await.map(Some),
                Err(err) => Err(TreasuryError::Treasury { source: err }),
            }
        })
    }

    fn update(
        &self,
        _id: AssetId,
        _version: u64,
    ) -> BoxFuture<Result<Option<AssetData>, Self::Error>> {
        Box::pin(async { Ok(None) })
    }
}

async fn asset_data_from_url(url: Url) -> Result<AssetData, TreasuryError> {
    match url.scheme() {
        "file" => match url.to_file_path() {
            Err(()) => Err(TreasuryError::UrlError { url }),
            Ok(path) => match tokio::runtime::Handle::try_current() {
                Err(_) => match std::fs::read(&path) {
                    Ok(data) => Ok(AssetData {
                        bytes: data.into_boxed_slice(),
                        version: 0,
                    }),
                    Err(err) => Err(TreasuryError::IoError {
                        source: err,
                        path: path.into_boxed_path(),
                    }),
                },
                Ok(runtime) => {
                    let result = runtime
                        .spawn_blocking(move || match std::fs::read(&path) {
                            Ok(data) => Ok(AssetData {
                                bytes: data.into_boxed_slice(),
                                version: 0,
                            }),
                            Err(err) => Err(TreasuryError::IoError {
                                source: err,
                                path: path.into_boxed_path(),
                            }),
                        })
                        .await;
                    match result {
                        Ok(Ok(data)) => Ok(data),
                        Ok(Err(err)) => Err(err),
                        Err(err) => Err(TreasuryError::JoinError { source: err }),
                    }
                }
            },
        },
        _ => Err(TreasuryError::UnsupportedUrlScheme { url }),
    }
}
