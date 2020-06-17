use {
    crate::source::{Source, SourceError},
    alloc::{boxed::Box, vec::Vec},
    maybe_sync::{BoxFuture, Rc},
    reqwest::{Client, IntoUrl, StatusCode},
};

/// Asset source that treats asset key as URL and fetches data from it.
/// Based on `reqwest` crate.
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "reqwest")))]
#[derive(Debug)]
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
}

impl<U> Source<U> for ReqwestSource
where
    U: IntoUrl + Clone + 'static,
{
    fn read(&self, url: &U) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
        let request = self.client.get(url.clone()).send();

        Box::pin(async move {
            let response = request.await.map_err(|err| {
                log::debug!("Error fetchin asset: {}", err);
                SourceError::NotFound
            })?;
            let status = response.status();
            match status {
                StatusCode::OK => {
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|err| SourceError::Error(Rc::new(err)))?;
                    Ok(bytes.as_ref().to_vec())
                }
                StatusCode::NO_CONTENT | StatusCode::MOVED_PERMANENTLY | StatusCode::NOT_FOUND => {
                    Err(SourceError::NotFound)
                }
                _ => {
                    log::warn!("Unexpected status: {}", status);
                    Err(SourceError::NotFound)
                }
            }
        })
    }
}
