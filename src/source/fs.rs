use {
    crate::source::{Source, SourceError},
    std::{path::{Path, PathBuf}, sync::Arc},
    futures_core::future::BoxFuture,
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
    P: AsRef<Path> + ?Sized,
{
    fn read(&self, path: &P) -> BoxFuture<'_, Result<Vec<u8>, SourceError>> {
        let path = self.root.join(path.as_ref());
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
