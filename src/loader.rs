use {
    crate::{
        asset::{Asset, AssetBuild},
        key::{hash_key, Key},
        source::{AssetData, Source},
        NotFound,
    },
    ahash::RandomState,
    futures::future::{BoxFuture, TryFutureExt as _},
    hashbrown::hash_map::{HashMap, RawEntryMut},
    parking_lot::Mutex,
    std::{
        any::Any,
        fmt::{self, Debug, Display},
        future::Future,
        hash::{BuildHasher, Hasher},
        pin::Pin,
        sync::Arc,
        task::{Context, Poll, Waker},
    },
    tracing::Instrument,
    uuid::Uuid,
};

macro_rules! assets_inner {
    ($sources:ident, $random_state:ident, $count:tt) => {{
        {
            let sources = $sources;
            let random_state = $random_state;
            (move || -> Arc<Inner<[Arc<Mutex<HashMap<Key, AssetEntry>>>]>> {
                let shards: Vec<_> = (0..$count * 4)
                    .map(|_| Arc::new(Mutex::new(HashMap::new())))
                    .collect();

                Arc::new(Inner {
                    sources,
                    random_state,
                    cache: std::convert::TryInto::<
                        [Arc<Mutex<HashMap<Key, AssetEntry>>>; $count * 4],
                    >::try_into(shards)
                    .unwrap_or_else(|_| panic!()),
                })
            })()
        }
    }};
}

/// This is default number of shards per CPU for shared hash map of asset states.
const DEFAULT_SHARDS_PER_CPU: usize = 8;

#[derive(Clone)]
#[repr(transparent)]
pub struct Error(Arc<dyn std::error::Error + Send + Sync>);

impl Error {
    fn new<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Error(Arc::new(error))
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&*self.0, f)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&*self.0, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&*self.0)
    }
}

trait AnySource: Send + Sync + 'static {
    fn load(&self, uuid: &Uuid) -> BoxFuture<Result<Option<AssetData>, Error>>;
    fn update(&self, uuid: &Uuid, version: u64) -> BoxFuture<Result<Option<AssetData>, Error>>;
}

impl<S> AnySource for S
where
    S: Source,
{
    fn load(&self, uuid: &Uuid) -> BoxFuture<Result<Option<AssetData>, Error>> {
        let fut = Source::load(self, uuid);
        Box::pin(fut.map_err(Error::new))
    }

    fn update(&self, uuid: &Uuid, version: u64) -> BoxFuture<Result<Option<AssetData>, Error>> {
        let fut = Source::update(self, uuid, version);
        Box::pin(fut.map_err(Error::new))
    }
}

struct Data {
    bytes: Box<[u8]>,
    version: u64,
    source: usize,
}

async fn load_asset(sources: &[Box<dyn AnySource>], uuid: &Uuid) -> Result<Option<Data>, Error> {
    for (index, source) in sources.iter().enumerate() {
        if let Some(asset) = source.load(uuid).await? {
            return Ok(Some(Data {
                bytes: asset.bytes,
                version: asset.version,
                source: index,
            }));
        }
    }
    Ok(None)
}

/// Builder for [`Loader`].
/// Allows configure asset loader with required [`Source`]s.
pub struct LoaderBuilder {
    num_shards: usize,
    sources: Vec<Box<dyn AnySource>>,
}

impl LoaderBuilder {
    /// Returns new [`LoaderBuilder`] without asset sources.
    pub fn new() -> Self {
        let num_cpus = num_cpus::get();
        let num_shards = DEFAULT_SHARDS_PER_CPU * num_cpus;

        LoaderBuilder {
            num_shards,
            sources: Vec::new(),
        }
    }

    /// Adds provided source to the loader.
    pub fn add(&mut self, source: impl Source) -> &mut Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Adds provided source to the loader.
    pub fn with(mut self, source: impl Source) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Sets number of shards for the loader.
    ///
    /// Actual number of shards will be bumped to the next power of two
    /// and limited to 512.
    ///
    /// This is low-level optimization tweaking function.
    /// Default value should be sufficient most use cases.
    pub fn set_num_shards(&mut self, num_shards: usize) -> &mut Self {
        self.num_shards = num_shards;
        self
    }

    /// Sets number of shards for the loader.
    ///
    /// Actual number of shards will be bumped to the next power of two
    /// and limited to 512.
    ///
    /// This is low-level optimization tweaking function.
    /// Default value should be sufficient most use cases.
    pub fn with_num_shards(mut self, num_shards: usize) -> Self {
        self.num_shards = num_shards;
        self
    }

    /// Builds and returns new [`Loader`] instance.
    pub fn build(self) -> Loader {
        let random_state = RandomState::new();
        let sources: Arc<[_]> = self.sources.into();

        let inner = match self.num_shards {
            0..=1 => assets_inner!(sources, random_state, 1),
            0..=2 => assets_inner!(sources, random_state, 2),
            0..=4 => assets_inner!(sources, random_state, 4),
            0..=8 => assets_inner!(sources, random_state, 8),
            0..=16 => assets_inner!(sources, random_state, 16),
            0..=32 => assets_inner!(sources, random_state, 32),
            0..=64 => assets_inner!(sources, random_state, 64),
            0..=128 => assets_inner!(sources, random_state, 128),
            0..=256 => assets_inner!(sources, random_state, 256),
            _ => assets_inner!(sources, random_state, 512),
        };

        Loader { inner }
    }
}

/// Virtual storage for all available assets.
#[derive(Clone)]
pub struct Loader {
    inner: Arc<Inner<[Arc<Mutex<HashMap<Key, AssetEntry, RandomState>>>]>>,
}

enum StateTyped<A: Asset> {
    #[allow(dead_code)]
    Asset {
        asset: A,
        version: u64,
        source: usize,
    },
    Decoded {
        decoded: Option<A::Decoded>,
        version: u64,
        source: usize,
    },
}

enum StateErased {
    Unloaded,
    Typed(Box<dyn Any + Send + Sync>),
    Missing,
    Error(Error),
}

struct AssetEntry {
    state: StateErased,
    wakers: Vec<Waker>,
}

enum AssetResultInner<A: Asset> {
    Asset(A),
    Error(Error),
    Missing,
    Decoded {
        uuid: Uuid,
        key_hash: u64,
        shard: Arc<Mutex<HashMap<Key, AssetEntry>>>,
    },
}

#[derive(Debug)]
pub struct AssetResultPoisoned;

impl Display for AssetResultPoisoned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("`AssetResult` poisoned by panic")
    }
}

impl std::error::Error for AssetResultPoisoned {}

#[repr(transparent)]
pub struct AssetResult<A: Asset>(AssetResultInner<A>);

impl<A> AssetResult<A>
where
    A: Asset,
{
    pub fn get_optional<B>(&mut self, builder: &mut B) -> Result<Option<&A>, Error>
    where
        A: AssetBuild<B>,
    {
        if let AssetResultInner::Decoded {
            uuid,
            key_hash,
            shard,
        } = &self.0
        {
            let mut locked_shard = shard.lock();
            let entry = locked_shard
                .raw_entry_mut()
                .from_hash(*key_hash, |k| k.eq_key::<A>(uuid));

            match entry {
                RawEntryMut::Vacant(_) => unreachable!(),
                RawEntryMut::Occupied(mut entry) => match &mut entry.get_mut().state {
                    StateErased::Typed(typed) => {
                        let typed: &mut StateTyped<A> = typed.downcast_mut().unwrap();

                        match typed {
                            StateTyped::Decoded {
                                decoded,
                                version,
                                source,
                            } => match decoded.take() {
                                Some(decoded) => match A::build(decoded, builder) {
                                    Ok(asset) => {
                                        *typed = StateTyped::Asset {
                                            asset: asset.clone(),
                                            version: *version,
                                            source: *source,
                                        };
                                        drop(locked_shard);
                                        self.0 = AssetResultInner::Asset(asset.clone());
                                    }
                                    Err(err) => {
                                        let err = Error::new(err);
                                        entry.get_mut().state = StateErased::Error(err.clone());
                                        drop(locked_shard);
                                        self.0 = AssetResultInner::Error(err.clone());
                                    }
                                },
                                None => {
                                    let err = Error::new(AssetResultPoisoned);
                                    entry.get_mut().state = StateErased::Error(err.clone());
                                    drop(locked_shard);
                                    self.0 = AssetResultInner::Error(err.clone());
                                }
                            },
                            StateTyped::Asset { asset, .. } => {
                                let asset = asset.clone();
                                drop(locked_shard);
                                self.0 = AssetResultInner::Asset(asset);
                            }
                        }
                    }
                    StateErased::Error(err) => {
                        let err = err.clone();
                        drop(locked_shard);
                        self.0 = AssetResultInner::Error(err);
                    }
                    StateErased::Unloaded => unreachable!(),
                    StateErased::Missing => unreachable!(),
                },
            }
        }

        match &self.0 {
            AssetResultInner::Missing => Ok(None),
            AssetResultInner::Asset(asset) => Ok(Some(asset)),
            AssetResultInner::Error(err) => Err(err.clone()),
            AssetResultInner::Decoded { .. } => unreachable!(),
        }
    }

    pub fn get<B>(&mut self, builder: &mut B) -> Result<&A, Error>
    where
        A: AssetBuild<B>,
    {
        self.get_optional(builder)?
            .ok_or_else(|| Error::new(NotFound))
    }
}

enum AssetHandleInner<A> {
    Asset(A),
    Error(Error),
    Missing,
    Pending {
        uuid: Uuid,
        key_hash: u64,
        shard: Arc<Mutex<HashMap<Key, AssetEntry>>>,
    },
}

#[repr(transparent)]
pub struct AssetHandle<A>(AssetHandleInner<A>);

impl<A> Unpin for AssetHandle<A> {}

impl<A> Future for AssetHandle<A>
where
    A: Asset,
{
    type Output = AssetResult<A>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.get_mut();

        match &me.0 {
            AssetHandleInner::Asset(asset) => {
                Poll::Ready(AssetResult(AssetResultInner::Asset(asset.clone())))
            }
            AssetHandleInner::Error(err) => {
                Poll::Ready(AssetResult(AssetResultInner::Error(err.clone())))
            }
            AssetHandleInner::Missing => Poll::Ready(AssetResult(AssetResultInner::Missing)),
            AssetHandleInner::Pending {
                uuid,
                key_hash,
                shard,
            } => {
                let mut locked_shard = shard.lock();
                let asset_entry = locked_shard
                    .raw_entry_mut()
                    .from_hash(*key_hash, |k| k.eq_key::<A>(uuid));

                match asset_entry {
                    RawEntryMut::Occupied(mut entry) => {
                        let entry = entry.get_mut();
                        match &entry.state {
                            StateErased::Error(err) => {
                                let err = err.clone();
                                drop(locked_shard);
                                me.0 = AssetHandleInner::Error(err.clone());
                                Poll::Ready(AssetResult(AssetResultInner::Error(err)))
                            }
                            StateErased::Missing => {
                                drop(locked_shard);
                                me.0 = AssetHandleInner::Missing;
                                Poll::Ready(AssetResult(AssetResultInner::Missing))
                            }
                            StateErased::Unloaded => {
                                entry.wakers.push(ctx.waker().clone());
                                Poll::Pending
                            }
                            StateErased::Typed(typed) => {
                                let typed: &StateTyped<A> = typed.downcast_ref().unwrap();
                                match typed {
                                    StateTyped::Asset { asset, .. } => {
                                        let asset = asset.clone();
                                        drop(locked_shard);
                                        me.0 = AssetHandleInner::Asset(asset.clone());
                                        Poll::Ready(AssetResult(AssetResultInner::Asset(asset)))
                                    }
                                    StateTyped::Decoded { .. } => {
                                        drop(locked_shard);
                                        Poll::Ready(AssetResult(AssetResultInner::Decoded {
                                            uuid: *uuid,
                                            key_hash: *key_hash,
                                            shard: shard.clone(),
                                        }))
                                    }
                                }
                            }
                        }
                    }
                    RawEntryMut::Vacant(_) => {
                        unreachable!()
                    }
                }
            }
        }
    }
}

struct Inner<T: ?Sized> {
    sources: Arc<[Box<dyn AnySource>]>,
    random_state: RandomState,
    cache: T,
}

impl Loader {
    /// Returns [`LoaderBuilder`] instance
    pub fn builder() -> LoaderBuilder {
        LoaderBuilder::new()
    }

    /// Reads raw bytes with provided key
    pub fn read(&self, uuid: &Uuid) -> impl Future<Output = Result<Box<[u8]>, Error>> {
        let inner = Arc::clone(&self.inner);
        let uuid = *uuid;
        async move {
            Ok(load_asset(&inner.sources, &uuid)
                .await?
                .ok_or_else(|| Error::new(NotFound))?
                .bytes)
        }
    }

    /// Load asset with specified uuid and returns handle
    /// that can be used to access assets once it is loaded.
    ///
    /// It asset was previously requested it will not be re-loaded,
    /// but handle to shared state will be returned instead,
    /// even if first load was not successful or different format was used.
    #[tracing::instrument(skip(self))]
    pub fn load<A>(&self, uuid: &Uuid) -> AssetHandle<A>
    where
        A: Asset,
    {
        // Hash asset key.
        let mut hasher = self.inner.random_state.build_hasher();
        hash_key::<A, _>(uuid, &mut hasher);
        let key_hash = hasher.finish();

        // Use asset key hash to pick a shard.
        // It will always pick same shard for same key.
        let shards_len = self.inner.cache.len();
        let shard = &self.inner.cache[(key_hash as usize % shards_len)];

        // Lock picked shard.
        let mut locked_shard = shard.lock();

        // Find an entry into sharded hashmap.
        let asset_entry = locked_shard
            .raw_entry_mut()
            .from_hash(key_hash, |k| k.eq_key::<A>(uuid));

        match asset_entry {
            RawEntryMut::Occupied(entry) => match &entry.get().state {
                // Already queried. See status.
                StateErased::Error(err) => AssetHandle(AssetHandleInner::Error(err.clone())),
                StateErased::Missing => AssetHandle(AssetHandleInner::Missing),
                StateErased::Unloaded => AssetHandle(AssetHandleInner::Pending {
                    uuid: *uuid,
                    key_hash,
                    shard: shard.clone(),
                }),
                StateErased::Typed(typed) => {
                    let typed: &StateTyped<A> = <dyn Any>::downcast_ref(&**typed).unwrap();
                    match typed {
                        StateTyped::Asset { asset, .. } => {
                            AssetHandle(AssetHandleInner::Asset(asset.clone()))
                        }
                        StateTyped::Decoded { .. } => AssetHandle(AssetHandleInner::Pending {
                            uuid: *uuid,
                            key_hash,
                            shard: shard.clone(),
                        }),
                    }
                }
            },
            RawEntryMut::Vacant(entry) => {
                let asset_key = Key::new::<A>(*uuid);
                // Register query
                let _ = entry.insert_hashed_nocheck(
                    key_hash,
                    asset_key.clone(),
                    AssetEntry {
                        state: StateErased::Unloaded,
                        wakers: Vec::new(),
                    },
                );
                drop(locked_shard);

                tokio::spawn({
                    let uuid = *uuid;
                    let inner = self.inner.clone();
                    let shard = shard.clone();

                    async move {
                        match load_asset(&inner.sources, &uuid).await {
                            Ok(Some(data)) => {
                                tracing::debug!("Asset data for `{}` loaded", uuid);

                                match A::decode(data.bytes, &Loader { inner }).await {
                                    Ok(decoded) => {
                                        let mut locked_shard = shard.lock();
                                        let asset_entry = locked_shard
                                            .raw_entry_mut()
                                            .from_hash(key_hash, |k| k.eq_key::<A>(&uuid));

                                        match asset_entry {
                                            RawEntryMut::Vacant(_) => {
                                                tracing::trace!("Asset already removed");
                                            }
                                            RawEntryMut::Occupied(mut entry) => {
                                                match &mut entry.get_mut().state {
                                                    StateErased::Unloaded => {
                                                        entry.get_mut().state = StateErased::Typed(
                                                            Box::new(StateTyped::<A>::Decoded {
                                                                decoded: Some(decoded),
                                                                version: data.version,
                                                                source: data.source,
                                                            }),
                                                        );
                                                        let wakers = std::mem::replace(
                                                            &mut entry.get_mut().wakers,
                                                            Vec::new(),
                                                        );
                                                        for waker in wakers {
                                                            waker.wake();
                                                        }
                                                        drop(locked_shard);
                                                    }
                                                    _ => panic!("Unexpected asset state"),
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        let mut locked_shard = shard.lock();
                                        let asset_entry = locked_shard
                                            .raw_entry_mut()
                                            .from_hash(key_hash, |k| k.eq_key::<A>(&uuid));

                                        match asset_entry {
                                            RawEntryMut::Vacant(_) => {
                                                tracing::trace!("Asset already removed");
                                            }
                                            RawEntryMut::Occupied(mut entry) => {
                                                match &mut entry.get_mut().state {
                                                    StateErased::Unloaded => {
                                                        entry.get_mut().state =
                                                            StateErased::Error(Error::new(err));
                                                        let wakers = std::mem::replace(
                                                            &mut entry.get_mut().wakers,
                                                            Vec::new(),
                                                        );
                                                        for waker in wakers {
                                                            waker.wake();
                                                        }
                                                        drop(locked_shard);
                                                    }
                                                    _ => panic!("Unexpected asset state"),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::warn!("Asset data for `{}` loaded", uuid);

                                let mut locked_shard = shard.lock();
                                let asset_entry = locked_shard
                                    .raw_entry_mut()
                                    .from_hash(key_hash, |k| k.eq_key::<A>(&uuid));

                                match asset_entry {
                                    RawEntryMut::Vacant(_) => {
                                        tracing::trace!("Asset already removed");
                                    }
                                    RawEntryMut::Occupied(mut entry) => {
                                        match &mut entry.get_mut().state {
                                            StateErased::Unloaded => {
                                                entry.get_mut().state = StateErased::Missing;
                                                let wakers = std::mem::replace(
                                                    &mut entry.get_mut().wakers,
                                                    Vec::new(),
                                                );
                                                for waker in wakers {
                                                    waker.wake();
                                                }
                                                drop(locked_shard);
                                            }
                                            _ => panic!("Unexpected asset state"),
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                let mut locked_shard = shard.lock();
                                let asset_entry = locked_shard
                                    .raw_entry_mut()
                                    .from_hash(key_hash, |k| k.eq_key::<A>(&uuid));

                                match asset_entry {
                                    RawEntryMut::Vacant(_) => {
                                        tracing::trace!("Asset already removed");
                                    }
                                    RawEntryMut::Occupied(mut entry) => {
                                        match &mut entry.get_mut().state {
                                            StateErased::Unloaded => {
                                                entry.get_mut().state = StateErased::Error(err);
                                                let wakers = std::mem::replace(
                                                    &mut entry.get_mut().wakers,
                                                    Vec::new(),
                                                );
                                                for waker in wakers {
                                                    waker.wake();
                                                }
                                                drop(locked_shard);
                                            }
                                            _ => panic!("Unexpected asset state"),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    .in_current_span()
                });

                AssetHandle(AssetHandleInner::Pending {
                    uuid: *uuid,
                    key_hash,
                    shard: shard.clone(),
                })
            }
        }
    }
}
