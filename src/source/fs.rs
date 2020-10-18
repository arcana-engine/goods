use {
    crate::{
        ready,
        source::{Source, SourceError},
    },
    futures_core::future::BoxFuture,
    std::{path::PathBuf, sync::Arc},
};

/// Asset source that treats asset key as relative file path,
/// joins it with root path and loads asset data from file.
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "fs")))]
#[derive(Debug)]
pub struct FileSource {
    root: PathBuf,
}

impl FileSource {
    /// Create new source with specified root path
    pub fn new(root: PathBuf) -> Self {
        #[cfg(feature = "trace")]
        tracing::info!("New file asset source. Root: {}", root.display());
        FileSource { root }
    }
}

impl<P> Source<P> for FileSource
where
    P: AsRef<str> + ?Sized,
{
    fn read(&self, path_or_url: &P) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
        let path_or_url: &str = path_or_url.as_ref();

        let path = if let Some(stripped) = path_or_url.strip_prefix("file://") {
            let path = if let Some(file_path) = stripped.strip_prefix('/') {
                file_path
            } else if let Some(localhost_path) = stripped.strip_prefix("localhost/") {
                localhost_path
            } else {
                return Box::pin(ready(Err(SourceError::NotFound)));
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

        #[cfg(feature = "trace")]
        tracing::debug!("Fetching asset file at {}", path.display());
        let result = match std::fs::read(path) {
            Ok(bytes) => {
                #[cfg(feature = "trace")]
                tracing::trace!("File loaded. {} bytes", bytes.len());
                Ok(bytes)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(feature = "trace")]
                tracing::debug!("File not found");
                Err(SourceError::NotFound)
            }
            Err(err) => {
                #[cfg(feature = "trace")]
                tracing::debug!("File loading error: {}", err);
                Err(SourceError::Error(Arc::new(err)))
            }
        };

        Box::pin(async move { result })
    }
}
