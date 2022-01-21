use std::convert::Infallible;

use smallvec::SmallVec;

use crate::{
    key::{hash_path_key, PathKey},
    AssetId, TypedAssetId,
};

use {
    crate::{
        asset::{Asset, AssetBuild},
        key::{hash_id_key, IdKey},
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
};

#[derive(Clone, Copy)]
pub enum Key<'a> {
    Path(&'a str),
    Id(AssetId),
}

impl Debug for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Path(path) => Debug::fmt(path, f),
            Key::Id(id) => Debug::fmt(id, f),
        }
    }
}

impl Display for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Path(path) => Debug::fmt(path, f),
            Key::Id(id) => Debug::fmt(id, f),
        }
    }
}

impl<'a, S> From<&'a S> for Key<'a>
where
    S: AsRef<str> + ?Sized,
{
    fn from(s: &'a S) -> Self {
        Key::Path(s.as_ref())
    }
}

impl From<AssetId> for Key<'_> {
    fn from(id: AssetId) -> Self {
        Key::Id(id)
    }
}

/// Trait to signal that asset build is trivial. E.g. asset is decoded directly from bytes and `AssetBuild::build` simply returns the value.
pub trait TrivialAssetBuild: AssetBuild<(), BuildError = Infallible> {}

impl<A> TrivialAssetBuild for A where A: AssetBuild<(), BuildError = Infallible> {}

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
        self.0.source()
    }
}

trait AnySource: Send + Sync + 'static {
    fn find(&self, path: &str, asset: &str) -> BoxFuture<Option<AssetId>>;
    fn load(&self, id: AssetId) -> BoxFuture<Result<Option<AssetData>, Error>>;
    fn update(&self, id: AssetId, version: u64) -> BoxFuture<Result<Option<AssetData>, Error>>;
}

impl<S> AnySource for S
where
    S: Source,
{
    fn find(&self, path: &str, asset: &str) -> BoxFuture<Option<AssetId>> {
        let fut = Source::find(self, path, asset);
        Box::pin(fut)
    }

    fn load(&self, id: AssetId) -> BoxFuture<Result<Option<AssetData>, Error>> {
        let fut = Source::load(self, id);
        Box::pin(fut.map_err(Error::new))
    }

    fn update(&self, id: AssetId, version: u64) -> BoxFuture<Result<Option<AssetData>, Error>> {
        let fut = Source::update(self, id, version);
        Box::pin(fut.map_err(Error::new))
    }
}

struct Data {
    bytes: Box<[u8]>,
    version: u64,
    source: usize,
}

type WakersVec = SmallVec<[Waker; 4]>;

/// Builder for [`Loader`].
/// Allows configure asset loader with required [`Source`]s.
pub struct LoaderBuilder {
    num_shards: usize,
    sources: Vec<Box<dyn AnySource>>,
}

impl Default for LoaderBuilder {
    fn default() -> Self {
        Self::new()
    }
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

        let shards: Vec<_> = (0..self.num_shards)
            .map(|_| Arc::new(Mutex::new(HashMap::new())))
            .collect();

        let path_shards: Vec<_> = (0..self.num_shards)
            .map(|_| Arc::new(Mutex::new(HashMap::new())))
            .collect();

        Loader {
            sources,
            random_state,
            cache: shards.into(),
            path_cache: path_shards.into(),
        }
    }
}

type Shard = Arc<Mutex<HashMap<IdKey, AssetState, RandomState>>>;

type PathShard = Arc<Mutex<HashMap<PathKey, PathAssetState, RandomState>>>;

/// Virtual storage for all available assets.
#[derive(Clone)]
pub struct Loader {
    sources: Arc<[Box<dyn AnySource>]>,
    random_state: RandomState,
    cache: Arc<[Shard]>,
    path_cache: Arc<[PathShard]>,
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

enum AssetState {
    /// Not yet loaded asset.
    Unloaded { wakers: WakeOnDrop },
    /// Contains `StateTyped<A>` with specific `A`
    Typed(Box<dyn Any + Send + Sync>),
    /// All sources reported that asset is missing.
    Missing,
    /// Source reported loading error.
    Error(Error),
}

enum PathAssetState {
    /// Not yet loaded asset.
    Unloaded { wakers: WakeOnDrop },

    /// Asset is loaded. Lookup main entry by this id.
    Loaded { id: AssetId },

    /// All sources reported that asset is missing.
    Missing,
}

enum AssetResultInner<A: Asset> {
    Asset(A),
    Error(Error),
    Missing,
    Decoded {
        id: AssetId,
        key_hash: u64,
        shard: Shard,
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

pub struct AssetResult<A: Asset> {
    key: Arc<str>,
    inner: AssetResultInner<A>,
}

impl<A> AssetResultInner<A>
where
    A: Asset,
{
    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// Returns `Ok(None)` if asset is not found.
    pub fn build_optional<B>(&mut self, builder: &mut B) -> Result<Option<&A>, Error>
    where
        A: AssetBuild<B>,
    {
        if let AssetResultInner::Decoded {
            id,
            key_hash,
            shard,
        } = &self
        {
            let mut locked_shard = shard.lock();
            let entry = locked_shard
                .raw_entry_mut()
                .from_hash(*key_hash, |k| k.eq_key::<A>(*id));

            match entry {
                RawEntryMut::Vacant(_) => unreachable!(),
                RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                    AssetState::Typed(typed) => {
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
                                        *self = AssetResultInner::Asset(asset);
                                    }
                                    Err(err) => {
                                        let err = Error::new(err);
                                        *entry.get_mut() = AssetState::Error(err.clone());
                                        drop(locked_shard);
                                        *self = AssetResultInner::Error(err);
                                    }
                                },
                                None => {
                                    let err = Error::new(AssetResultPoisoned);
                                    *entry.get_mut() = AssetState::Error(err.clone());
                                    drop(locked_shard);
                                    *self = AssetResultInner::Error(err);
                                }
                            },
                            StateTyped::Asset { asset, .. } => {
                                let asset = asset.clone();
                                drop(locked_shard);
                                *self = AssetResultInner::Asset(asset);
                            }
                        }
                    }
                    AssetState::Error(err) => {
                        let err = err.clone();
                        drop(locked_shard);
                        *self = AssetResultInner::Error(err);
                    }
                    AssetState::Unloaded { .. } => unreachable!(),
                    AssetState::Missing => unreachable!(),
                },
            }
        }

        match self {
            AssetResultInner::Missing => Ok(None),
            AssetResultInner::Asset(asset) => Ok(Some(asset)),
            AssetResultInner::Error(err) => Err(err.clone()),
            AssetResultInner::Decoded { .. } => unreachable!(),
        }
    }
}

impl<A> AssetResult<A>
where
    A: Asset,
{
    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// Returns `Ok(None)` if asset is not found.
    pub fn build_optional<B>(&mut self, builder: &mut B) -> Result<Option<&A>, Error>
    where
        A: AssetBuild<B>,
    {
        self.inner.build_optional(builder)
    }

    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors or if asset is not found.
    pub fn build<B>(&mut self, builder: &mut B) -> Result<&A, Error>
    where
        A: AssetBuild<B>,
    {
        match self.inner.build_optional(builder)? {
            None => Err(Error::new(NotFound {
                key: self.key.clone(),
            })),
            Some(asset) => Ok(asset),
        }
    }

    /// Returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// This function is only usable for `TrivialAssetBuild` implementors.
    /// All `TrivialAsset`s implement `TrivialAssetBuild`.
    pub fn get_optional(&mut self) -> Result<Option<&A>, Error>
    where
        A: TrivialAssetBuild,
    {
        self.build_optional(&mut ())
    }

    /// Returns asset.
    /// Returns `Err` if asset decoding or build errors or if asset is not found.
    /// This function is only usable for `TrivialAssetBuild` implementors.
    /// All `TrivialAsset`s implement `TrivialAssetBuild`.
    pub fn get(&mut self) -> Result<&A, Error>
    where
        A: TrivialAssetBuild,
    {
        match self.inner.build_optional(&mut ())? {
            None => Err(Error::new(NotFound {
                key: self.key.clone(),
            })),
            Some(asset) => Ok(asset),
        }
    }
}

enum AssetHandleInner<A> {
    Asset(A),
    Error(Error),
    Missing,
    Searching {
        path: Arc<str>,
        key_hash: u64,
        path_shard: PathShard,
        shards: Arc<[Shard]>,
        random_state: RandomState,
    },
    Loading {
        id: AssetId,
        key_hash: u64,
        shard: Shard,
    },
}

pub struct AssetHandle<A> {
    key: Arc<str>,
    inner: AssetHandleInner<A>,
}

impl<A> Unpin for AssetHandle<A> {}

impl<A> Future for AssetHandle<A>
where
    A: Asset,
{
    type Output = AssetResult<A>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.get_mut();

        loop {
            match &me.inner {
                AssetHandleInner::Asset(asset) => {
                    return Poll::Ready(AssetResult {
                        key: me.key.clone(),
                        inner: AssetResultInner::Asset(asset.clone()),
                    })
                }
                AssetHandleInner::Error(err) => {
                    return Poll::Ready(AssetResult {
                        key: me.key.clone(),
                        inner: AssetResultInner::Error(err.clone()),
                    })
                }
                AssetHandleInner::Searching {
                    path,
                    path_shard,
                    key_hash,
                    shards,
                    random_state,
                } => {
                    let mut locked_shard = path_shard.lock();
                    let asset_entry = locked_shard
                        .raw_entry_mut()
                        .from_hash(*key_hash, |k| k.eq_key::<A>(path));

                    match asset_entry {
                        RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                            PathAssetState::Missing => {
                                drop(locked_shard);
                                me.inner = AssetHandleInner::Missing;
                                return Poll::Ready(AssetResult {
                                    key: me.key.clone(),
                                    inner: AssetResultInner::Missing,
                                });
                            }
                            PathAssetState::Unloaded { wakers } => {
                                wakers.push(ctx.waker().clone());
                                return Poll::Pending;
                            }
                            PathAssetState::Loaded { id } => {
                                let id = *id;
                                let mut hasher = random_state.build_hasher();
                                hash_id_key::<A, _>(id, &mut hasher);
                                let key_hash = hasher.finish();

                                let shard = shards[key_hash as usize % shards.len()].clone();

                                drop(locked_shard);

                                me.inner = AssetHandleInner::Loading {
                                    id,
                                    key_hash,
                                    shard,
                                };
                            }
                        },
                        RawEntryMut::Vacant(_) => {
                            unreachable!()
                        }
                    }
                }
                AssetHandleInner::Missing => {
                    return Poll::Ready(AssetResult {
                        key: me.key.clone(),
                        inner: AssetResultInner::Missing,
                    })
                }
                AssetHandleInner::Loading {
                    id,
                    key_hash,
                    shard,
                } => {
                    let mut locked_shard = shard.lock();
                    let asset_entry = locked_shard
                        .raw_entry_mut()
                        .from_hash(*key_hash, |k| k.eq_key::<A>(*id));

                    return match asset_entry {
                        RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                            AssetState::Error(err) => {
                                let err = err.clone();
                                drop(locked_shard);
                                me.inner = AssetHandleInner::Error(err.clone());
                                Poll::Ready(AssetResult {
                                    key: me.key.clone(),
                                    inner: AssetResultInner::Error(err),
                                })
                            }
                            AssetState::Missing => {
                                drop(locked_shard);
                                me.inner = AssetHandleInner::Missing;
                                Poll::Ready(AssetResult {
                                    key: me.key.clone(),
                                    inner: AssetResultInner::Missing,
                                })
                            }
                            AssetState::Unloaded { wakers } => {
                                wakers.push(ctx.waker().clone());
                                Poll::Pending
                            }
                            AssetState::Typed(typed) => {
                                let typed: &StateTyped<A> = typed.downcast_ref().unwrap();
                                match typed {
                                    StateTyped::Asset { asset, .. } => {
                                        let asset = asset.clone();
                                        drop(locked_shard);
                                        me.inner = AssetHandleInner::Asset(asset.clone());
                                        Poll::Ready(AssetResult {
                                            key: me.key.clone(),
                                            inner: AssetResultInner::Asset(asset),
                                        })
                                    }
                                    StateTyped::Decoded { .. } => {
                                        drop(locked_shard);
                                        Poll::Ready(AssetResult {
                                            key: me.key.clone(),
                                            inner: AssetResultInner::Decoded {
                                                id: *id,
                                                key_hash: *key_hash,
                                                shard: shard.clone(),
                                            },
                                        })
                                    }
                                }
                            }
                        },
                        RawEntryMut::Vacant(_) => {
                            unreachable!()
                        }
                    };
                }
            }
        }
    }
}

impl Loader {
    /// Returns [`LoaderBuilder`] instance
    pub fn builder() -> LoaderBuilder {
        LoaderBuilder::new()
    }

    /// Load asset with specified id and returns handle
    /// that can be used to access assets once it is loaded.
    ///
    /// If asset was previously requested it will not be re-loaded,
    /// but handle to shared state will be returned instead,
    /// even if first load was not successful or different format was used.
    pub fn load_typed<A>(&self, id: TypedAssetId<A>) -> AssetHandle<A>
    where
        A: Asset,
    {
        self.load::<A, AssetId>(id.id)
    }

    /// Load asset with specified key (path or id) and returns handle
    /// that can be used to access assets once it is loaded.
    ///
    /// If asset was previously requested it will not be re-loaded,
    /// but handle to shared state will be returned instead,
    /// even if first load was not successful or different format was used.
    pub fn load<'a, A, K>(&self, key: K) -> AssetHandle<A>
    where
        A: Asset,
        K: Into<Key<'a>>,
    {
        match key.into() {
            Key::Path(path) => {
                // Hash asset path key.
                let mut hasher = self.random_state.build_hasher();
                hash_path_key::<A, _>(path, &mut hasher);
                let key_hash = hasher.finish();

                // Use asset key hash to pick a shard.
                // It will always pick same shard for same key.
                let shards_len = self.path_cache.len();
                let path_shard = &self.path_cache[(key_hash as usize % shards_len)];

                // Lock picked shard.
                let mut locked_shard = path_shard.lock();

                // Find an entry into sharded hashmap.
                let asset_entry = locked_shard
                    .raw_entry_mut()
                    .from_hash(key_hash, |k| k.eq_key::<A>(path));

                match asset_entry {
                    RawEntryMut::Occupied(entry) => {
                        let path = entry.key().path.clone();
                        match entry.get() {
                            // Already queried. See status.
                            PathAssetState::Missing => AssetHandle {
                                key: path,
                                inner: AssetHandleInner::Missing,
                            },
                            PathAssetState::Unloaded { .. } => {
                                drop(locked_shard);

                                AssetHandle {
                                    key: path.clone(),
                                    inner: AssetHandleInner::Searching {
                                        path: path,
                                        key_hash,
                                        path_shard: path_shard.clone(),
                                        shards: self.cache.clone(),
                                        random_state: self.random_state.clone(),
                                    },
                                }
                            }
                            PathAssetState::Loaded { id } => {
                                let id = *id;
                                drop(locked_shard);

                                // Hash asset key.
                                let mut hasher = self.random_state.build_hasher();
                                hash_id_key::<A, _>(id, &mut hasher);
                                let key_hash = hasher.finish();

                                // Use asset key hash to pick a shard.
                                // It will always pick same shard for same key.
                                let shards_len = self.cache.len();
                                let shard = &self.cache[(key_hash as usize % shards_len)];

                                AssetHandle {
                                    key: path,
                                    inner: AssetHandleInner::Loading {
                                        id,
                                        key_hash,
                                        shard: shard.clone(),
                                    },
                                }
                            }
                        }
                    }
                    RawEntryMut::Vacant(entry) => {
                        let asset_key = PathKey::new::<A>(path.into());
                        let path = asset_key.path.clone();

                        // Register query
                        let _ = entry.insert_hashed_nocheck(
                            key_hash,
                            asset_key,
                            PathAssetState::Unloaded {
                                wakers: WakeOnDrop::new(),
                            },
                        );
                        drop(locked_shard);

                        let loader = self.clone();
                        let path_shard = path_shard.clone();

                        let handle = AssetHandle {
                            key: path.clone(),
                            inner: AssetHandleInner::Searching {
                                path: path.clone(),
                                key_hash,
                                path_shard: path_shard.clone(),
                                shards: self.cache.clone(),
                                random_state: self.random_state.clone(),
                            },
                        };

                        tokio::spawn(
                            async move {
                                find_asset_task::<A>(&loader, path_shard, key_hash, &path).await;
                            }
                            .in_current_span(),
                        );

                        handle
                    }
                }
            }
            Key::Id(id) => {
                // Hash asset key.
                let mut hasher = self.random_state.build_hasher();
                hash_id_key::<A, _>(id, &mut hasher);
                let key_hash = hasher.finish();

                // Use asset key hash to pick a shard.
                // It will always pick same shard for same key.
                let shards_len = self.cache.len();
                let shard = &self.cache[(key_hash as usize % shards_len)];

                // Lock picked shard.
                let mut locked_shard = shard.lock();

                // Find an entry into sharded hashmap.
                let asset_entry = locked_shard
                    .raw_entry_mut()
                    .from_hash(key_hash, |k| k.eq_key::<A>(id));

                let key = id.to_string().into();

                match asset_entry {
                    RawEntryMut::Occupied(entry) => {
                        match entry.get() {
                            // Already queried. See status.
                            AssetState::Error(err) => AssetHandle {
                                key,
                                inner: AssetHandleInner::Error(err.clone()),
                            },
                            AssetState::Missing => AssetHandle {
                                key,
                                inner: AssetHandleInner::Missing,
                            },
                            AssetState::Unloaded { .. } => AssetHandle {
                                key,
                                inner: AssetHandleInner::Loading {
                                    id,
                                    key_hash,
                                    shard: shard.clone(),
                                },
                            },
                            AssetState::Typed(typed) => {
                                let typed: &StateTyped<A> =
                                    <dyn Any>::downcast_ref(&**typed).unwrap();
                                match typed {
                                    StateTyped::Asset { asset, .. } => AssetHandle {
                                        key,
                                        inner: AssetHandleInner::Asset(asset.clone()),
                                    },
                                    StateTyped::Decoded { .. } => AssetHandle {
                                        key,
                                        inner: AssetHandleInner::Loading {
                                            id,
                                            key_hash,
                                            shard: shard.clone(),
                                        },
                                    },
                                }
                            }
                        }
                    }
                    RawEntryMut::Vacant(entry) => {
                        let asset_key = IdKey::new::<A>(id);

                        // Register query
                        let _ = entry.insert_hashed_nocheck(
                            key_hash,
                            asset_key,
                            AssetState::Unloaded {
                                wakers: WakeOnDrop::new(),
                            },
                        );
                        drop(locked_shard);

                        let loader = self.clone();
                        let shard = shard.clone();

                        let handle = AssetHandle {
                            key,
                            inner: AssetHandleInner::Loading {
                                id,
                                key_hash,
                                shard: shard.clone(),
                            },
                        };

                        tokio::spawn(
                            async move {
                                load_asset_task::<A>(&loader, shard, key_hash, id).await;
                            }
                            .in_current_span(),
                        );

                        handle
                    }
                }
            }
        }
    }
}

async fn load_asset_task<A: Asset>(loader: &Loader, shard: Shard, key_hash: u64, id: AssetId) {
    let new_state = match load_asset(&loader.sources, id).await {
        Err(err) => AssetState::Error(err),
        Ok(None) => AssetState::Missing,
        Ok(Some(data)) => {
            let result = A::decode(data.bytes, loader).await;

            match result {
                Err(err) => AssetState::Error(Error::new(err)),
                Ok(decoded) => {
                    let typed = StateTyped::<A>::Decoded {
                        decoded: Some(decoded),
                        version: data.version,
                        source: data.source,
                    };

                    AssetState::Typed(Box::new(typed))
                }
            }
        }
    };

    // Asset not found. Change state and notify waters.
    let mut locked_shard = shard.lock();

    let entry = locked_shard
        .raw_entry_mut()
        .from_hash(key_hash, |k| k.eq_key::<A>(id));

    match entry {
        RawEntryMut::Vacant(_) => {
            unreachable!("No other code could change the state")
        }
        RawEntryMut::Occupied(mut entry) => {
            let entry = entry.get_mut();
            match entry {
                AssetState::Unloaded { .. } => {
                    *entry = new_state;
                }
                _ => unreachable!("No other code could change the state"),
            }
        }
    }
}

async fn find_asset_task<A: Asset>(
    loader: &Loader,
    path_shard: PathShard,
    key_hash: u64,
    path: &str,
) {
    match find_asset::<A>(&loader.sources, path).await {
        None => {
            // Asset not found. Change state and notify waters.
            let mut locked_shard = path_shard.lock();

            let entry = locked_shard
                .raw_entry_mut()
                .from_hash(key_hash, |k| k.eq_key::<A>(path));

            match entry {
                RawEntryMut::Vacant(_) => {
                    unreachable!("No other code could change the state")
                }
                RawEntryMut::Occupied(mut entry) => {
                    let entry = entry.get_mut();
                    match entry {
                        PathAssetState::Unloaded { .. } => {
                            *entry = PathAssetState::Missing;
                        }
                        _ => unreachable!("No other code could change the state"),
                    }
                }
            }
        }
        Some(id) => {
            let mut moving_wakers = WakeOnDrop::new();

            {
                // Asset found. Change the state
                let mut locked_shard = path_shard.lock();

                let entry = locked_shard
                    .raw_entry_mut()
                    .from_hash(key_hash, |k| k.eq_key::<A>(path));

                match entry {
                    RawEntryMut::Vacant(_) => {
                        unreachable!("No other code could change the state")
                    }
                    RawEntryMut::Occupied(mut entry) => {
                        let state = entry.get_mut();
                        match state {
                            PathAssetState::Unloaded { wakers } => {
                                // Decide what to do with wakers later.
                                moving_wakers.append(&mut wakers.vec);
                                *state = PathAssetState::Loaded { id };
                            }
                            _ => unreachable!("No other code could change the state"),
                        }
                    }
                }
            }

            // Hash asset key.
            let mut hasher = loader.random_state.build_hasher();
            hash_id_key::<A, _>(id, &mut hasher);
            let key_hash = hasher.finish();

            // Check ID entry.
            let shard_idx = key_hash as usize % loader.cache.len();
            let shard = loader.cache[shard_idx].clone();

            {
                let mut locked_shard = shard.lock();

                let entry = locked_shard
                    .raw_entry_mut()
                    .from_hash(key_hash, |k| k.eq_key::<A>(id));

                match entry {
                    RawEntryMut::Vacant(entry) => {
                        // Asset was not requested by ID yet.
                        let asset_key = IdKey::new::<A>(id);

                        // Register query
                        let _ = entry.insert_hashed_nocheck(
                            key_hash,
                            asset_key,
                            AssetState::Unloaded {
                                wakers: moving_wakers,
                            }, // Put wakers here.
                        );
                    }
                    RawEntryMut::Occupied(mut entry) => {
                        match entry.get_mut() {
                            AssetState::Unloaded { wakers } => {
                                // Move wakers to ID entry.
                                wakers.append(&mut moving_wakers.vec);
                            }
                            _ => {
                                // Loading is complete one way or another.
                                // Wake wakers from path entry.
                            }
                        }
                        return;
                    }
                }
            }

            // Proceed loading by ID.
            load_asset_task::<A>(loader, shard, key_hash, id).await;
        }
    }
}

async fn load_asset(sources: &[Box<dyn AnySource>], id: AssetId) -> Result<Option<Data>, Error> {
    for (index, source) in sources.iter().enumerate() {
        if let Some(asset) = source.load(id).await? {
            return Ok(Some(Data {
                bytes: asset.bytes,
                version: asset.version,
                source: index,
            }));
        }
    }
    Ok(None)
}

async fn find_asset<A: Asset>(sources: &[Box<dyn AnySource>], path: &str) -> Option<AssetId> {
    for source in sources {
        if let Some(id) = source.find(path, A::name()).await {
            return Some(id);
        }
    }
    None
}

// Convenient type to wake wakers on scope exit.
struct WakeOnDrop {
    vec: WakersVec,
}

impl WakeOnDrop {
    fn new() -> Self {
        WakeOnDrop {
            vec: WakersVec::new(),
        }
    }

    fn append(&mut self, v: &mut WakersVec) {
        self.vec.append(v);
    }

    fn push(&mut self, waker: Waker) {
        self.vec.push(waker);
    }
}

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        for waker in self.vec.drain(..) {
            waker.wake()
        }
    }
}
