use {
    goods::AssetNotFound,
    reqwest::{Client, IntoUrl, StatusCode},
    std::future::Future,
};

#[cfg(target_arch = "wasm32")]
use futures_core::future::LocalBoxFuture;

#[cfg(not(target_arch = "wasm32"))]
use futures_core::future::BoxFuture;

/// Asset source that treats asset key as URL and fetches data from it.
/// Based on `reqwest` crate.
#[derive(Debug, Default)]
pub struct ReqwestSource {
    client: Client,
}

impl ReqwestSource {
    pub fn new() -> Self {
        ReqwestSource::default()
    }

    pub fn with_client(client: Client) -> Self {
        ReqwestSource { client }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl goods::AutoLocalSource for ReqwestSource {}

impl ReqwestSource {
    async fn read_impl(
        &self,
        request: impl Future<Output = Result<reqwest::Response, reqwest::Error>>,
    ) -> eyre::Result<Box<[u8]>> {
        let response = request.await.map_err(|_err| {
            #[cfg(feature = "trace")]
            tracing::debug!("Error fetching asset: {}", _err);
            AssetNotFound
        })?;
        let status = response.status();
        match status {
            StatusCode::OK => {
                let bytes = response.bytes().await?;
                Ok(bytes.as_ref().into())
            }
            StatusCode::NO_CONTENT | StatusCode::MOVED_PERMANENTLY | StatusCode::NOT_FOUND => {
                Err(AssetNotFound.into())
            }
            _ => {
                #[cfg(feature = "trace")]
                tracing::warn!("Unexpected status: {}", status);
                Err(AssetNotFound.into())
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<U> goods::LocalSource<U> for ReqwestSource
where
    U: IntoUrl + Clone + 'static,
{
    fn read_local(&self, url: &U) -> LocalBoxFuture<'_, eyre::Result<Box<[u8]>>> {
        let request = self.client.get(url.clone()).send();
        Box::pin(self.read_impl(request))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<U> goods::Source<U> for ReqwestSource
where
    U: IntoUrl + Clone + 'static,
{
    fn read(&self, url: &U) -> BoxFuture<'_, eyre::Result<Box<[u8]>>> {
        let request = self.client.get(url.clone()).send();
        Box::pin(self.read_impl(request))
    }
}
