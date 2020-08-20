use {
    crate::{
        ready,
        source::{Source, SourceError},
    },
    alloc::{boxed::Box, sync::Arc},
    futures_core::future::BoxFuture,
};

/// Quasi-source that decodes data directly from URL with `data:` scheme.
#[derive(Debug)]
pub struct DataUrlSource;

impl<U> Source<U> for DataUrlSource
where
    U: AsRef<str> + ?Sized,
{
    fn read(&self, url: &U) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
        let url = url.as_ref();

        let result = if !url.starts_with("data:") {
            #[cfg(feature = "trace")]
            tracing::trace!("Not a data URL");
            Err(SourceError::NotFound)
        } else {
            if let Some(comma) = url["data:".len()..].find(',') {
                let data = &url["data:".len() + comma + 1..];
                match base64::decode_config(data, base64::URL_SAFE.decode_allow_trailing_bits(true))
                {
                    Ok(bytes) => Ok(bytes),
                    Err(err) => {
                        #[cfg(feature = "trace")]
                        tracing::warn!("failed to decode bese64 payload in data URL");
                        Err(SourceError::Error(Arc::new(err)))
                    }
                }
            } else {
                #[cfg(feature = "trace")]
                tracing::warn!("missing comma in data URL");
                Err(SourceError::NotFound)
            }
        };

        Box::pin(ready(result))
    }
}
