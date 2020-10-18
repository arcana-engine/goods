use {
    crate::{
        asset::{Asset, AssetDefaultFormat, Format, PhantomContext},
        channel::{slot, Sender},
        error::Error,
        handle::{AnyHandle, Handle},
        key::Key,
        process::{AnyProcess, Process, Processor},
        registry::Registry,
        source::SourceError,
        spawn::{LocalSpawn, Spawn, SpawnError},
    },
    alloc::{boxed::Box, sync::Arc, vec::Vec},
    core::{
        any::TypeId,
        borrow::Borrow,
        fmt::{self, Debug},
        future::Future,
        hash::{BuildHasher, Hash, Hasher},
    },
    futures_core::future::{BoxFuture, LocalBoxFuture},
    hashbrown::hash_map::{Entry, HashMap, RawEntryMut},
};

#[cfg(feature = "std")]
use parking_lot::Mutex;

#[cfg(not(feature = "std"))]
use spin::Mutex;

/// Asset cache.
/// This type is main entry point for asset loading.
/// Caches loaded assets and provokes loading work for new assets.
pub struct Cache<K> {
    registry: Registry<K>,
    inner: Arc<Inner<K, dyn EitherSpawn>>,
}

trait EitherSpawn: Debug + Send + Sync {
    fn can_spawn_local(&self) -> bool {
        false
    }

    fn spawn(&self, f: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        self.spawn_local(f)
    }

    fn spawn_local(&self, _f: LocalBoxFuture<'static, ()>) -> Result<(), SpawnError> {
        unreachable!()
    }
}

struct SpawnWrapper<S> {
    spawn: S,
}

impl<S> Debug for SpawnWrapper<S>
where
    S: Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.spawn, fmt)
    }
}

impl<S> EitherSpawn for SpawnWrapper<S>
where
    S: Spawn + Debug + Send + Sync,
{
    fn spawn(&self, f: BoxFuture<'static, ()>) -> Result<(), SpawnError> {
        self.spawn.spawn(f)
    }
}

struct LocalSpawnWrapper<S> {
    spawn: S,
}

impl<S> Debug for LocalSpawnWrapper<S>
where
    S: Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.spawn, fmt)
    }
}

impl<S> EitherSpawn for LocalSpawnWrapper<S>
where
    S: LocalSpawn + Debug + Send + Sync,
{
    fn can_spawn_local(&self) -> bool {
        true
    }

    fn spawn_local(&self, f: LocalBoxFuture<'static, ()>) -> Result<(), SpawnError> {
        self.spawn.spawn(f)
    }
}

struct Inner<K, S: ?Sized> {
    cache: Mutex<HashMap<(TypeId, K), AnyHandle>>,
    processor: Processor,
    spawn: S,
}

impl<K> Debug for Cache<K> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("goods::Cache")
            .field("registry", &self.registry)
            .field("spawn", &&self.inner.spawn)
            .finish()
    }
}

impl<K> Clone for Cache<K> {
    fn clone(&self) -> Self {
        Cache {
            registry: self.registry.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<K> Cache<K> {
    /// Creates new asset cache.
    /// Assets will be loaded from provided `registry`.
    /// Loading tasks will be sent to `Loader`.
    pub fn new<S>(registry: Registry<K>, spawn: S) -> Self
    where
        S: Spawn + Send + Sync + 'static,
    {
        #[cfg(feature = "trace")]
        tracing::info!("Creating new asset cache");

        Cache {
            registry,
            inner: Arc::new(Inner {
                cache: Mutex::default(),
                processor: Processor::new(),
                spawn: SpawnWrapper { spawn },
            }),
        }
    }

    /// Creates new asset cache.
    /// Assets will be loaded from provided `registry`.
    /// Loading tasks will be sent to `Loader`.
    pub fn local<S>(registry: Registry<K>, spawn: S) -> Self
    where
        S: LocalSpawn + Send + Sync + 'static,
    {
        #[cfg(feature = "trace")]
        tracing::info!("Creating new asset cache");

        Cache {
            registry,
            inner: Arc::new(Inner {
                cache: Mutex::default(),
                processor: Processor::new(),
                spawn: LocalSpawnWrapper { spawn },
            }),
        }
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses default asset format for decoding.
    pub fn load<A>(&self, key: K) -> Handle<A>
    where
        K: Key,
        A: AssetDefaultFormat<K>,
    {
        self.load_with_format(key, A::DefaultFormat::default())
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses provided asset format for decoding.
    #[cfg_attr(feature = "trace", tracing::instrument(skip(self)))]
    pub fn load_with_format<A, F>(&self, key: K, format: F) -> Handle<A>
    where
        K: Key,
        A: Asset,
        F: Format<A, K>,
    {
        let tid = TypeId::of::<A>();

        let mut lock = self.inner.cache.lock();

        match lock.entry((tid, key.clone())) {
            Entry::Occupied(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("Asset was already requested");
                let any = entry.get().clone();
                drop(lock);
                any.downcast::<A>().unwrap()
            }
            Entry::Vacant(entry) => {
                #[cfg(feature = "trace")]
                tracing::trace!("New asset requested");
                let handle = Handle::new();
                entry.insert(handle.clone().into());
                drop(lock);

                if self.inner.spawn.can_spawn_local() {
                    let task: LocalBoxFuture<'_, _> =
                        if TypeId::of::<A::Context>() == TypeId::of::<PhantomContext>() {
                            Box::pin(load_asset_with_phantom_context(
                                key.clone(),
                                self.registry.clone().read_local(key),
                                format,
                                self.clone(),
                                handle.clone(),
                            ))
                        } else {
                            Box::pin(load_asset(
                                key.clone(),
                                self.registry.clone().read_local(key),
                                format,
                                self.clone(),
                                self.inner.processor.sender::<A>(),
                                handle.clone(),
                            ))
                        };

                    if let Err(SpawnError) = self.inner.spawn.spawn_local(task) {
                        handle.set(Err(Error::SpawnError));
                    }
                } else {
                    let task: BoxFuture<'_, _> =
                        if TypeId::of::<A::Context>() == TypeId::of::<PhantomContext>() {
                            Box::pin(load_asset_with_phantom_context(
                                key.clone(),
                                self.registry.clone().read(key),
                                format,
                                self.clone(),
                                handle.clone(),
                            ))
                        } else {
                            Box::pin(load_asset(
                                key.clone(),
                                self.registry.clone().read(key),
                                format,
                                self.clone(),
                                self.inner.processor.sender::<A>(),
                                handle.clone(),
                            ))
                        };

                    if let Err(SpawnError) = self.inner.spawn.spawn(task) {
                        handle.set(Err(Error::SpawnError));
                    }
                }

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
        let mut lock = self.inner.cache.lock();

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

#[cfg_attr(feature = "trace", tracing::instrument(skip(loading, process_sender)))]
pub(crate) async fn load_asset<A, F, K, L>(
    key: K,
    loading: L,
    format: F,
    cache: Cache<K>,
    process_sender: Sender<Box<dyn AnyProcess<A::Context> + Send>>,
    handle: Handle<A>,
) where
    A: Asset,
    F: Format<A, K>,
    K: Debug,
    L: Future<Output = Result<Vec<u8>, SourceError>> + 'static,
{
    handle.set(
        async move {
            let bytes = loading.await?;
            let decode = format.decode(key, bytes, &cache);
            drop(cache);
            let repr: A::Repr = decode.await.map_err(|err| Error::Format(Arc::new(err)))?;
            let (slot, setter) = slot::<A::BuildFuture>();
            process_sender.send(Box::new(Process::<A> { repr, setter }));
            slot.await.await.map_err(|err| Error::Asset(Arc::new(err)))
        }
        .await,
    )
}

#[cfg_attr(feature = "trace", tracing::instrument(skip(loading)))]
pub(crate) async fn load_asset_with_phantom_context<A, F, K, L>(
    key: K,
    loading: L,
    format: F,
    cache: Cache<K>,
    handle: Handle<A>,
) where
    A: Asset,
    F: Format<A, K>,
    K: Debug,
    L: Future<Output = Result<Vec<u8>, SourceError>> + 'static,
{
    debug_assert_eq!(TypeId::of::<A::Context>(), TypeId::of::<PhantomContext>());

    handle.set(
        async move {
            let bytes = loading.await?;
            #[cfg(feature = "trace")]
            tracing::trace!("Raw asset date loaded. {} bytes", bytes.len());
            let decode = format.decode(key, bytes, &cache);
            drop(cache);
            let repr = decode.await.map_err(|err| Error::Format(Arc::new(err)))?;
            #[cfg(feature = "trace")]
            tracing::trace!("Asset decoded");
            let build = A::build(repr, unsafe {
                &mut *{ &mut PhantomContext as *mut _ as *mut A::Context }
            });
            let asset = build.await.map_err(|err| Error::Asset(Arc::new(err)))?;
            #[cfg(feature = "trace")]
            tracing::trace!("Asset loaded");
            Ok(asset)
        }
        .await,
    );
    #[cfg(feature = "trace")]
    tracing::trace!("Asset handle set");
}
