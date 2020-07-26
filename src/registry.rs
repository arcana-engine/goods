use {
    crate::source::{LocalSource, Source, SourceError},
    alloc::{boxed::Box, sync::Arc, vec::Vec},
    core::fmt::{self, Debug},
};

/// Builder for source registry.
pub struct RegistryBuilder<K: ?Sized> {
    sources: Vec<Box<dyn Source<K>>>,
    local_sources: Vec<Box<dyn LocalSource<K>>>,
}

impl<K> Default for RegistryBuilder<K>
where
    K: ?Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> RegistryBuilder<K>
where
    K: ?Sized,
{
    /// Create new empty builder.
    pub fn new() -> Self {
        RegistryBuilder {
            sources: Vec::new(),
            local_sources: Vec::new(),
        }
    }

    /// Add source to the registry builder.
    pub fn with(mut self, storage: impl Source<K>) -> Self {
        self.add(storage);
        self
    }

    /// Add source to the registry builder.
    pub fn with_local(mut self, storage: impl LocalSource<K>) -> Self {
        self.add_local(storage);
        self
    }

    /// Add source to the registry builder.
    pub fn add(&mut self, storage: impl Source<K>) -> &mut Self {
        self.sources.push(Box::new(storage));
        self
    }

    /// Add source to the registry builder.
    pub fn add_local(&mut self, storage: impl LocalSource<K>) -> &mut Self {
        self.local_sources.push(Box::new(storage));
        self
    }

    /// Build registry.
    pub fn build(self) -> Registry<K> {
        Registry {
            sources: self.sources.into(),
            local_sources: self.local_sources.into(),
        }
    }
}

/// Collection of registered sources.
/// Used by `Cache` to load new assets.
pub struct Registry<K: ?Sized> {
    sources: Arc<[Box<dyn Source<K>>]>,
    local_sources: Arc<[Box<dyn LocalSource<K>>]>,
}

impl<K> Clone for Registry<K>
where
    K: ?Sized,
{
    fn clone(&self) -> Self {
        Registry {
            sources: self.sources.clone(),
            local_sources: self.local_sources.clone(),
        }
    }
}

impl<K> Registry<K>
where
    K: 'static,
{
    /// Create registry builder.
    pub fn builder() -> RegistryBuilder<K> {
        RegistryBuilder::new()
    }

    /// Try to read data for asset with specified key.
    /// This method will try to read asset from all registered sources one-by-one.
    /// Returns as soon as first source returns anything except `SourceError::NotFound`.
    ///
    /// This method ignores sources that return non-send futures.
    pub async fn read(self, key: K) -> Result<Vec<u8>, SourceError> {
        for storage in &*self.sources {
            match storage.read(&key).await {
                Err(SourceError::NotFound) => continue,
                result => return result,
            }
        }
        Err(SourceError::NotFound)
    }

    /// Try to read data for asset with specified key.
    /// This method will try to read asset from all registered sources one-by-one.
    /// Returns as soon as first source returns anything except `SourceError::NotFound`.
    ///
    /// This method reads from sources that return non-send futures.
    pub async fn read_local(self, key: K) -> Result<Vec<u8>, SourceError> {
        for source in &*self.sources {
            match source.read(&key).await {
                Err(SourceError::NotFound) => continue,
                result => return result,
            }
        }
        for source in &*self.local_sources {
            match source.read(&key).await {
                Err(SourceError::NotFound) => continue,
                result => return result,
            }
        }
        Err(SourceError::NotFound)
    }
}

impl<K> Debug for Registry<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Registry")
            .field("sources", &self.sources)
            .finish()
    }
}
