use {
    futures_core::future::BoxFuture,
    goods::{AssetNotFound, AutoLocalSource, Source},
    std::future::ready,
};

/// Quasi-source that decodes data directly from URL with `data:` scheme.
#[derive(Debug)]
pub struct DataUrlSource;

impl AutoLocalSource for DataUrlSource {}

impl<U> Source<U> for DataUrlSource
where
    U: AsRef<str>,
{
    fn read(&self, url: &U) -> BoxFuture<'static, eyre::Result<Box<[u8]>>> {
        let url = url.as_ref();

        let result = if !url.starts_with("data:") {
            #[cfg(feature = "trace")]
            tracing::trace!("Not a data URL");
            Err(AssetNotFound.into())
        } else if let Some(comma) = url["data:".len()..].find(',') {
            let data = &url["data:".len() + comma + 1..];
            match base64::decode_config(data, base64::URL_SAFE.decode_allow_trailing_bits(true)) {
                Ok(bytes) => Ok(bytes.into_boxed_slice()),
                Err(err) => {
                    #[cfg(feature = "trace")]
                    tracing::warn!("failed to decode bese64 payload in data URL");
                    Err(err.into())
                }
            }
        } else {
            #[cfg(feature = "trace")]
            tracing::warn!("missing comma in data URL");
            Err(AssetNotFound.into())
        };

        Box::pin(ready(result))
    }
}
