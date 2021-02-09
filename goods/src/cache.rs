use {
    crate::{
        asset::{Asset, PhantomContext},
        format::{AssetDefaultFormat, Format, LocalFormat},
        handle::{AnyHandle, AnyLocalHandle, Handle, LocalHandle},
        key::Key,
        process::{LocalProcessor, Processor},
        registry::{LocalRegistry, Registry},
    },
    hashbrown::hash_map::{Entry, HashMap, RawEntryMut},
    std::{
        any::TypeId,
        borrow::Borrow,
        cell::RefCell,
        fmt::{self, Debug},
        future::Future,
        hash::{BuildHasher, Hash, Hasher},
        rc::Rc,
        sync::{Arc, Mutex},
    },
};

/// Asset cache.
/// This type is main entry point for asset loading.
/// Caches loaded assets and provokes loading work for new assets.
pub struct Cache<K> {
    inner: Arc<Inner<K>>,
    registry: Registry<K>,
}

struct Inner<K> {
    cache: Mutex<HashMap<(TypeId, K), AnyHandle>>,
    processor: Processor,
}

impl<K> Debug for Cache<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("goods::Cache")
            .field("registry", &self.registry)
            .finish()
    }
}

impl<K> Clone for Cache<K> {
    fn clone(&self) -> Self {
        Cache {
            inner: self.inner.clone(),
            registry: self.registry.clone(),
        }
    }
}

impl<K> Cache<K> {
    /// Creates new asset cache.
    /// Assets will be loaded from provided `registry`.
    /// Loading tasks will be sent to `Loader`.
    pub fn new(registry: Registry<K>) -> Self {
        #[cfg(feature = "trace")]
        tracing::info!("Creating new asset cache");

        Cache {
            registry,
            inner: Arc::new(Inner {
                cache: Mutex::default(),
                processor: Processor::new(),
            }),
        }
    }

    /// Read raw asset bytes without caching.
    pub fn read(&self, key: K) -> impl Future<Output = eyre::Result<Box<[u8]>>> + Send + 'static
    where
        K: Key + Send + Sync,
    {
        let registry = self.registry.clone();
        async move { registry.read(&key).await }
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses default asset format for decoding.
    pub fn load<A>(&self, key: K) -> Handle<A>
    where
        K: Key + Send + Sync,
        A: AssetDefaultFormat + Send + Sync,
        A::DefaultFormat: Send,
        A::Repr: Send + Sync,
        A::BuildFuture: Send,
        A::DefaultFormat: Format<A::Repr, K> + Send,
    {
        self.load_with_format(key, A::DefaultFormat::default())
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses provided asset format for decoding.
    #[cfg_attr(feature = "trace", tracing::instrument(skip(self)))]
    pub fn load_with_format<A, F>(&self, key: K, format: F) -> Handle<A>
    where
        K: Key + Send + Sync,
        A: Asset + Send + Sync,
        A::Repr: Send,
        A::BuildFuture: Send,
        F: Format<A::Repr, K> + Send,
    {
        let tid = TypeId::of::<A>();

        let mut lock = self.inner.cache.lock().unwrap();

        match lock.entry((tid, key.clone())) {
            Entry::Occupied(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("Asset was already requested");
                let any = entry.get();
                let handle = unsafe { any.downcast::<A>() };
                drop(lock);

                handle
            }
            Entry::Vacant(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("New asset requested");

                let handle = Handle::from_future(load_asset(key, format, self.clone()));

                entry.insert(handle.clone().erase_type());
                drop(lock);

                handle
            }
        }
    }

    /// Removes asset cache with specified key if exists.
    /// Subsequent loads with that key will result in loading process started anew
    /// even if previously assets are still alive.
    /// Returns if asset cache was removed.
    pub fn remove<A, Q>(&self, key: &Q) -> bool
    where
        A: 'static,
        K: Key + Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut lock = self.inner.cache.lock().unwrap();

        // Raw entry API is used because it is impossible to construct type
        // borrowable from (TypeId, K) using `&Q`.
        let mut hasher = lock.hasher().build_hasher();

        // For equivalent Q and K their hashes must be the same.
        // This is identical to `HashMap::remove` requirement.
        (TypeId::of::<A>(), key).hash(&mut hasher);
        let hash = hasher.finish();

        let entry = lock.raw_entry_mut().from_hash(hash, |(tid, k)| {
            *tid == TypeId::of::<A>() && k.borrow() == key
        });

        match entry {
            RawEntryMut::Occupied(entry) => {
                entry.remove();
                true
            }
            _ => false,
        }
    }

    /// Process intermediate asset represnetations into assets.
    /// Calling this function will build all loaded assets whose context type is `C`.
    pub fn process<C: 'static>(&self, ctx: &mut C)
    where
        K: 'static,
    {
        self.inner.processor.run(ctx);
    }
}

#[cfg_attr(feature = "trace", tracing::instrument)]
async fn load_asset<A, F, K>(key: K, format: F, cache: Cache<K>) -> eyre::Result<A>
where
    A: Asset,
    A::Repr: Send,
    A::BuildFuture: Send,
    F: Format<A::Repr, K>,
    K: Key,
{
    #[cfg(feature = "trace")]
    tracing::trace!("Asset loading started");

    let bytes = cache.registry.read(&key).await?;
    #[cfg(feature = "trace")]
    tracing::trace!("Raw asset date loaded. {} bytes", bytes.len());

    let decode = format.decode(key, bytes, &cache);

    let repr = decode.await?;
    #[cfg(feature = "trace")]
    tracing::trace!("Asset decoded");

    let build = match try_build_with_phantom_context::<A>(repr) {
        Ok(build) => build,
        Err(repr) => cache.inner.processor.with_context::<A>(repr).await,
    };

    let asset = build.await?;

    #[cfg(feature = "trace")]
    tracing::trace!("Asset loaded");

    Ok(asset)
}

/// Asset cache.
/// This type is main entry point for asset loading.
/// Caches loaded assets and provokes loading work for new assets.
pub struct LocalCache<K> {
    inner: Rc<LocalInner<K>>,
    registry: LocalRegistry<K>,
}

struct LocalInner<K> {
    cache: RefCell<HashMap<(TypeId, K), AnyLocalHandle>>,
    processor: LocalProcessor,
}

impl<K> Debug for LocalCache<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("goods::LocalCache")
            .field("registry", &self.registry)
            .finish()
    }
}

impl<K> Clone for LocalCache<K> {
    fn clone(&self) -> Self {
        LocalCache {
            inner: self.inner.clone(),
            registry: self.registry.clone(),
        }
    }
}

impl<K> LocalCache<K> {
    /// Creates new asset cache.
    /// Assets will be loaded from provided `registry`.
    /// Loading tasks will be sent to `Loader`.
    pub fn new(registry: LocalRegistry<K>) -> Self {
        #[cfg(feature = "trace")]
        tracing::info!("Creating new asset cache");

        LocalCache {
            registry,
            inner: Rc::new(LocalInner {
                cache: RefCell::default(),
                processor: LocalProcessor::new(),
            }),
        }
    }

    /// Read raw asset bytes without caching.
    pub fn read(&self, key: K) -> impl Future<Output = eyre::Result<Box<[u8]>>> + 'static
    where
        K: Key,
    {
        let registry = self.registry.clone();
        async move { registry.read(&key).await }
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses default asset format for decoding.
    pub fn load<A>(&self, key: K) -> LocalHandle<A>
    where
        K: Key,
        A: AssetDefaultFormat,
        A::DefaultFormat: LocalFormat<A::Repr, K>,
    {
        self.load_with_format(key, A::DefaultFormat::default())
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses provided asset format for decoding.
    #[cfg_attr(feature = "trace", tracing::instrument(skip(self)))]
    pub fn load_with_format<A, F>(&self, key: K, format: F) -> LocalHandle<A>
    where
        K: Key,
        A: Asset,
        F: LocalFormat<A::Repr, K>,
    {
        let tid = TypeId::of::<A>();

        let mut lock = self.inner.cache.borrow_mut();

        match lock.entry((tid, key.clone())) {
            Entry::Occupied(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("Asset was already requested");
                let any = entry.get();
                let handle = unsafe { any.downcast::<A>() };
                drop(lock);

                handle
            }
            Entry::Vacant(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("New asset requested");

                let handle = LocalHandle::from_future(load_asset_local(key, format, self.clone()));

                entry.insert(handle.clone().erase_type());
                drop(lock);

                handle
            }
        }
    }

    /// Removes asset cache with specified key if exists.
    /// Subsequent loads with that key will result in loading process started anew
    /// even if previously assets are still alive.
    /// Returns if asset cache was removed.
    pub fn remove<A, Q>(&self, key: &Q) -> bool
    where
        A: 'static,
        K: Key + Borrow<Q>,
        Q: Hash + Eq,
    {
        let mut lock = self.inner.cache.borrow_mut();

        // Raw entry API is used because it is impossible to construct type
        // borrowable from (TypeId, K) using `&Q`.
        let mut hasher = lock.hasher().build_hasher();

        // For equivalent Q and K their hashes must be the same.
        // This is identical to `HashMap::remove` requirement.
        (TypeId::of::<A>(), key).hash(&mut hasher);
        let hash = hasher.finish();

        let entry = lock.raw_entry_mut().from_hash(hash, |(tid, k)| {
            *tid == TypeId::of::<A>() && k.borrow() == key
        });

        match entry {
            RawEntryMut::Occupied(entry) => {
                entry.remove();
                true
            }
            _ => false,
        }
    }

    /// Process intermediate asset represnetations into assets.
    /// Calling this function will build all loaded assets whose context type is `C`.
    pub fn process<C: 'static>(&self, ctx: &mut C)
    where
        K: 'static,
    {
        self.inner.processor.run(ctx);
    }
}

#[cfg_attr(feature = "trace", tracing::instrument)]
async fn load_asset_local<A, F, K>(key: K, format: F, cache: LocalCache<K>) -> eyre::Result<A>
where
    A: Asset,
    F: LocalFormat<A::Repr, K>,
    K: Key,
{
    #[cfg(feature = "trace")]
    tracing::trace!("Asset loading started");

    let bytes = cache.registry.read(&key).await?;
    #[cfg(feature = "trace")]
    tracing::trace!("Raw asset date loaded. {} bytes", bytes.len());

    let decode = format.decode_local(key, bytes, &cache);

    let repr = decode.await?;
    #[cfg(feature = "trace")]
    tracing::trace!("Asset decoded");

    let build = match try_build_with_phantom_context::<A>(repr) {
        Ok(build) => build,
        Err(repr) => cache.inner.processor.with_context::<A>(repr).await,
    };

    let asset = build.await?;

    #[cfg(feature = "trace")]
    tracing::trace!("Asset loaded");

    Ok(asset)
}

fn try_build_with_phantom_context<A: Asset>(repr: A::Repr) -> Result<A::BuildFuture, A::Repr> {
    if TypeId::of::<PhantomContext>() == TypeId::of::<A::Context>() {
        let mut ctx = PhantomContext;
        let ctx = unsafe { &mut *(&mut ctx as *mut PhantomContext as *mut A::Context) };
        Ok(A::build(repr, ctx))
    } else {
        Err(repr)
    }
}
