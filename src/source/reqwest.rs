use {
    crate::source::{Source, SourceError},
    alloc::{boxed::Box, sync::Arc, vec::Vec},
    core::future::Future,
    futures_core::future::BoxFuture,
    reqwest::{Client, IntoUrl, StatusCode},
};

/// Asset source that treats asset key as URL and fetches data from it.
/// Based on `reqwest` crate.
pub struct ReqwestSource {
    client: Client,
}

impl ReqwestSource {
    pub fn new() -> Self {
        ReqwestSource {
            client: Client::new(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        ReqwestSource { client }
    }

    pub fn read(
        &self,
        url: impl IntoUrl,
    ) -> impl Future<Output = Result<Vec<u8>, SourceError>> + 'static {
        let request = self.client.get(url).send();

        async move {
            let response = request.await.map_err(|err| {
                log::trace!("Error fetchin asset {}", err);
                SourceError::NotFound
            })?;
            let status = response.status();

            match status {
                StatusCode::OK => {
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|err| SourceError::Error(Arc::new(err)))?;
                    Ok(bytes.as_ref().to_vec())
                }
                StatusCode::NO_CONTENT | StatusCode::MOVED_PERMANENTLY | StatusCode::NOT_FOUND => {
                    Err(SourceError::NotFound)
                }
                _ => {
                    log::warn!("Unexpected status {}", status);
                    Err(SourceError::NotFound)
                }
            }
        }
    }
}

impl<U> Source<U> for ReqwestSource
where
    U: IntoUrl + Clone + 'static,
{
    fn read(&self, url: &U) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
        Box::pin(self.read(url.clone()))
    }
}
