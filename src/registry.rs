use {
    crate::{
        source::{Source, SourceError},
        sync::Ptr,
    },
    alloc::{boxed::Box, vec::Vec},
    core::fmt::{self, Debug},
};

/// Builder for source registry.
pub struct RegistryBuilder<K: ?Sized> {
    storages: Vec<Box<dyn Source<K>>>,
}

impl<K> RegistryBuilder<K>
where
    K: ?Sized,
{
    /// Create new empty builder.
    pub fn new() -> Self {
        RegistryBuilder {
            storages: Vec::new(),
        }
    }

    /// Add source to the registry builder.
    pub fn with(mut self, storage: impl Source<K>) -> Self {
        self.add(storage);
        self
    }

    /// Add source to the registry builder.
    pub fn add(&mut self, storage: impl Source<K>) -> &mut Self {
        self.storages.push(Box::new(storage));
        self
    }

    /// Build registry.
    pub fn build(self) -> Registry<K> {
        Registry {
            storages: self.storages.into(),
        }
    }
}

/// Collection of registered sources.
/// Used by `Cache` to load new assets.
pub struct Registry<K: ?Sized> {
    storages: Ptr<[Box<dyn Source<K>>]>,
}

impl<K> Clone for Registry<K>
where
    K: ?Sized,
{
    fn clone(&self) -> Self {
        Registry {
            storages: self.storages.clone(),
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
    pub async fn read(self, key: K) -> Result<Vec<u8>, SourceError> {
        for storage in &*self.storages {
            match storage.read(&key).await {
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
            .field("storages", &self.storages)
            .finish()
    }
}
