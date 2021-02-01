use {
    futures_core::future::BoxFuture,
    goods::{AssetNotFound, AutoLocalSource, Source},
    std::{future::ready, path::PathBuf},
};

/// Asset source that treats asset key as relative file path,
/// joins it with root path and loads asset data from file.
#[derive(Debug)]
pub struct FileSource {
    root: PathBuf,
}

impl AutoLocalSource for FileSource {}

impl FileSource {
    /// Create new source with specified root path
    pub fn new(root: PathBuf) -> Self {
        #[cfg(feature = "tracing")]
        tracing::info!("New file asset source. Root: {}", root.display());
        FileSource { root }
    }
}

impl<P> Source<P> for FileSource
where
    P: AsRef<str>,
{
    fn read(&self, path_or_url: &P) -> BoxFuture<'static, eyre::Result<Box<[u8]>>> {
        let path_or_url: &str = path_or_url.as_ref();

        let path = if let Some(stripped) = path_or_url.strip_prefix("file://") {
            let path = if let Some(file_path) = stripped.strip_prefix('/') {
                file_path
            } else if let Some(localhost_path) = stripped.strip_prefix("localhost/") {
                localhost_path
            } else {
                return Box::pin(ready(Err(AssetNotFound.into())));
            };
            #[cfg(feature = "urlencoding")]
            {
                match urlencoding::decode(path) {
                    Ok(decoded) => self.root.join(&decoded),
                    Err(err) => {
                        return Box::pin(ready(Err(SourceError::Error(Arc::new(err)))));
                    }
                }
            }
            #[cfg(not(feature = "urlencoding"))]
            {
                self.root.join(path)
            }
        } else {
            self.root.join(path_or_url)
        };
        let path = self.root.join(path);

        #[cfg(feature = "tracing")]
        tracing::debug!("Fetching asset file at {}", path.display());
        let result = match std::fs::read(path) {
            Ok(bytes) => {
                #[cfg(feature = "tracing")]
                tracing::trace!("File loaded. {} bytes", bytes.len());
                Ok(bytes.into_boxed_slice())
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(feature = "tracing")]
                tracing::debug!("File not found");
                Err(AssetNotFound.into())
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                tracing::debug!("File loading error: {}", err);
                Err(err.into())
            }
        };

        Box::pin(ready(result))
    }
}
