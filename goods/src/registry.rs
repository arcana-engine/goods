use {
    crate::source::{AssetNotFound, LocalSource, Source},
    std::{
        fmt::{self, Debug},
        rc::Rc,
        sync::Arc,
    },
};

/// Builder for source registry.
pub struct RegistryBuilder<K> {
    sources: Vec<Box<dyn Source<K>>>,
}

impl<K> Default for RegistryBuilder<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> RegistryBuilder<K> {
    /// Create new empty builder.
    pub fn new() -> Self {
        RegistryBuilder {
            sources: Vec::new(),
        }
    }

    /// Add source to the registry builder.
    pub fn with(mut self, storage: impl Source<K>) -> Self {
        self.add(storage);
        self
    }

    /// Add source to the registry builder.
    pub fn add(&mut self, storage: impl Source<K>) -> &mut Self {
        self.sources.push(Box::new(storage));
        self
    }

    /// Build registry.
    pub fn build(self) -> Registry<K> {
        Registry {
            sources: self.sources.into(),
        }
    }
}

/// Collection of registered sources.
/// Used by `Cache` to load new assets.
pub struct Registry<K> {
    sources: Arc<[Box<dyn Source<K>>]>,
}

impl<K> Clone for Registry<K> {
    fn clone(&self) -> Self {
        Registry {
            sources: self.sources.clone(),
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
    pub async fn read(&self, key: &K) -> eyre::Result<Box<[u8]>> {
        for storage in &*self.sources {
            match storage.read(&key).await {
                Err(err) if err.is::<AssetNotFound>() => continue,
                Err(err) => return Err(err),
                Ok(bytes) => return Ok(bytes),
            }
        }
        Err(AssetNotFound.into())
    }
}

impl<K> Debug for Registry<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Registry")
            .field("sources", &self.sources)
            .finish()
    }
}

/// Builder for source registry.
pub struct LocalRegistryBuilder<K> {
    sources: Vec<Box<dyn LocalSource<K>>>,
}

impl<K> Default for LocalRegistryBuilder<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> LocalRegistryBuilder<K> {
    /// Create new empty builder.
    pub fn new() -> Self {
        LocalRegistryBuilder {
            sources: Vec::new(),
        }
    }

    /// Add source to the registry builder.
    pub fn with(mut self, storage: impl LocalSource<K>) -> Self {
        self.add(storage);
        self
    }

    /// Add source to the registry builder.
    pub fn add(&mut self, storage: impl LocalSource<K>) -> &mut Self {
        self.sources.push(Box::new(storage));
        self
    }

    /// Build registry.
    pub fn build(self) -> LocalRegistry<K> {
        LocalRegistry {
            sources: self.sources.into(),
        }
    }
}

/// Collection of registered sources.
/// Used by `Cache` to load new assets.
pub struct LocalRegistry<K> {
    sources: Rc<[Box<dyn LocalSource<K>>]>,
}

impl<K> Clone for LocalRegistry<K> {
    fn clone(&self) -> Self {
        LocalRegistry {
            sources: self.sources.clone(),
        }
    }
}

impl<K> LocalRegistry<K>
where
    K: 'static,
{
    /// Create registry builder.
    pub fn builder() -> LocalRegistryBuilder<K> {
        LocalRegistryBuilder::new()
    }

    /// Try to read data for asset with specified key.
    /// This method will try to read asset from all registered sources one-by-one.
    /// Returns as soon as first source returns anything except `SourceError::NotFound`.
    ///
    /// This method ignores sources that return non-send futures.
    pub async fn read(&self, key: &K) -> eyre::Result<Box<[u8]>> {
        for storage in &*self.sources {
            match storage.read_local(&key).await {
                Err(err) if err.is::<AssetNotFound>() => continue,
                Err(err) => return Err(err),
                Ok(bytes) => return Ok(bytes),
            }
        }
        Err(AssetNotFound.into())
    }
}

impl<K> Debug for LocalRegistry<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Registry")
            .field("sources", &self.sources)
            .finish()
    }
}
