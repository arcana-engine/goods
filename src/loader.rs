use std::{
    any::Any,
    convert::Infallible,
    fmt::{self, Debug, Display},
    future::Future,
    hash::{BuildHasher, Hasher},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use ahash::RandomState;
use futures::future::{BoxFuture, TryFutureExt as _};
use hashbrown::hash_map::{HashMap, RawEntryMut};
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use tracing::Instrument;

use crate::{
    key::{hash_path_key, PathKey},
    AssetId, TypedAssetId,
};

use crate::{
    asset::{Asset, AssetBuild},
    key::{hash_id_key, IdKey},
    source::{AssetData, Source},
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

#[derive(thiserror::Error)]
pub struct NotFound {
    pub path: Option<Arc<str>>,
    pub id: Option<AssetId>,
}

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.path, &self.id) {
            (None, None) => f.write_str("Failed to load an asset. [No AssetId or path provided]"),
            (Some(path), None) => write!(f, "Failed to load asset '{}'", path),
            (None, Some(id)) => write!(f, "Failed to load asset '{}'", id),
            (Some(path), Some(id)) => write!(f, "Failed to load asset '{} @ {}'", id, path),
        }
    }
}

impl fmt::Debug for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
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

struct DecodedState<A: Asset> {
    decoded: Option<A::Decoded>,
    version: u64,
    source: usize,
}

enum StateTyped<A: Asset> {
    #[allow(dead_code)]
    Asset {
        asset: A,
        version: u64,
        source: usize,
    },
    Decoded {
        lock: Arc<spin::Mutex<DecodedState<A>>>, // Spin-lock is cheap. Expect near zero contention.
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

enum AssetResultState<A> {
    Asset(A),
    Error(Error),
    Missing,
    Decoded {
        // id: AssetId,
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

pub struct AssetResult<A> {
    path: Option<Arc<str>>,
    id: Option<AssetId>,
    state: AssetResultState<A>,
}

impl<A> AssetResultState<A>
where
    A: Asset,
{
    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// Returns `Ok(None)` if asset is not found.
    pub fn build_optional<B>(
        &mut self,
        id: Option<AssetId>,
        builder: &mut B,
    ) -> Result<Option<&A>, Error>
    where
        A: AssetBuild<B>,
    {
        if let AssetResultState::Decoded { key_hash, shard } = &self {
            let id = id.expect("This state can be reached only with known id");
            let mut locked_shard = shard.lock();
            let entry = locked_shard
                .raw_entry_mut()
                .from_hash(*key_hash, |k| k.eq_key::<A>(id));

            match entry {
                RawEntryMut::Vacant(_) => {
                    unreachable!("AssetResult existence guarantee entry is not vacant")
                }
                RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                    AssetState::Typed(typed) => {
                        let typed: &mut StateTyped<A> = typed.downcast_mut().unwrap();

                        match typed {
                            StateTyped::Decoded { lock } => {
                                // This spin-lock is for deadlock and poisoning avoidance.
                                let lock = lock.clone();

                                let (res, state) = MutexGuard::unlocked(&mut locked_shard, || {
                                    // Lock asset not under shard lock.
                                    let mut state = lock.lock();
                                    match state.decoded.take() {
                                        Some(decoded) => (
                                            Some((
                                                A::build(decoded, builder),
                                                state.version,
                                                state.source,
                                            )),
                                            state,
                                        ),
                                        None => (None, state),
                                    }
                                });

                                // Unlock asset under shard lock
                                drop(state);

                                // Visit entry again
                                let entry = locked_shard
                                    .raw_entry_mut()
                                    .from_hash(*key_hash, |k| k.eq_key::<A>(id));

                                match entry {
                                    RawEntryMut::Vacant(_) => unreachable!(
                                        "AssetResult existence guarantee entry is not vacant"
                                    ),
                                    RawEntryMut::Occupied(mut entry) => {
                                        match res {
                                            None => {
                                                match entry.get_mut() {
                                                    AssetState::Typed(typed) => {
                                                        let typed: &mut StateTyped<A> =
                                                            typed.downcast_mut().unwrap();
            
                                                        match typed {
                                                            StateTyped::Asset { asset, .. } => {
                                                                        let asset = asset.clone();
                                                                        drop(locked_shard);
                                                                        *self = AssetResultState::Asset(asset.clone());
                                                                    }
                                                            _ => unreachable!("Decode state was taken. Another thread finished building the asset"),
                                                        }
                                                    }
                                                    AssetState::Error(err) => {
                                                        // Already failed
                                                        let err = err.clone();
                                                        drop(locked_shard);
                                                        *self = AssetResultState::Error(err);
                                                    }
                                                    _ => unreachable!("It was in `Typed` state. It can be changed to `Error` only"),
                                                }
                                            }
                                            Some((Ok(asset), version, source)) => {
                                                match entry.get_mut() {
                                                    AssetState::Typed(typed) => {
                                                        let typed: &mut StateTyped<A> =
                                                            typed.downcast_mut().unwrap();
        
                                                        // Successful asset build
                                                        *typed = StateTyped::Asset {
                                                            asset: asset.clone(),
                                                            version,
                                                            source,
                                                        };
                                                        drop(locked_shard);
                                                        *self = AssetResultState::Asset(asset);
                                                    }
                                                    _ => unreachable!("Decode state was taken be this thread"),
                                                }

                                            }
                                            Some((Err(err), _, _)) => {
                                                // Build failed
                                                let err = Error::new(err);
                                                *entry.get_mut() = AssetState::Error(err.clone());
                                                drop(locked_shard);
                                                *self = AssetResultState::Error(err);
                                            }
                                        }
                                    }
                                }
                            }
                            StateTyped::Asset { asset, .. } => {
                                // Already built
                                let asset = asset.clone();
                                drop(locked_shard);
                                *self = AssetResultState::Asset(asset);
                            }
                        }
                    }
                    AssetState::Error(err) => {
                        // Already failed
                        let err = err.clone();
                        drop(locked_shard);
                        *self = AssetResultState::Error(err);
                    }
                    AssetState::Unloaded { .. } => unreachable!(),
                    AssetState::Missing => unreachable!(),
                },
            }
        }

        match self {
            AssetResultState::Missing => Ok(None),
            AssetResultState::Asset(asset) => Ok(Some(asset)),
            AssetResultState::Error(err) => Err(err.clone()),
            AssetResultState::Decoded { .. } => unreachable!(),
        }
    }
}

impl<A> AssetResult<A>
where
    A: Asset,
{
    /// Returns path if fetched by path.
    #[inline]
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns AssetId unless fetched by path and is not found.
    #[inline]
    pub fn id(&self) -> Option<AssetId> {
        self.id
    }

    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// Returns `Ok(None)` if asset is not found.
    #[inline]
    pub fn build_optional<B>(&mut self, builder: &mut B) -> Result<Option<&A>, Error>
    where
        A: AssetBuild<B>,
    {
        self.state.build_optional(self.id, builder)
    }

    /// Builds and returns asset.
    /// Returns `Err` if asset decoding or build errors or if asset is not found.
    #[inline]
    pub fn build<B>(&mut self, builder: &mut B) -> Result<&A, Error>
    where
        A: AssetBuild<B>,
    {
        match self.state.build_optional(self.id, builder)? {
            None => Err(Error::new(NotFound {
                path: self.path.clone(),
                id: self.id.clone(),
            })),
            Some(asset) => Ok(asset),
        }
    }

    /// Returns asset.
    /// Returns `Err` if asset decoding or build errors.
    /// This function is only usable for `TrivialAssetBuild` implementors.
    /// All `TrivialAsset`s implement `TrivialAssetBuild`.
    #[inline]
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
    #[inline]
    pub fn get(&mut self) -> Result<&A, Error>
    where
        A: TrivialAssetBuild,
    {
        self.build(&mut ())
    }
}

enum AssetHandleState<A> {
    Asset {
        asset: A,
    },
    Error {
        error: Error,
    },
    Missing,
    Searching {
        key_hash: u64,
        path_shard: PathShard,
        shards: Arc<[Shard]>,
        random_state: RandomState,
    },
    Loading {
        key_hash: u64,
        shard: Shard,
    },
}

/// Handle for a requested asset.
/// Can be polled manually or as a `Future`.
/// Resolves into `AssetResult<A>` which can be used
/// to build the asset instance, or just take if it is ready.
pub struct AssetHandle<A> {
    path: Option<Arc<str>>,
    id: Option<AssetId>,
    state: AssetHandleState<A>,
}

impl<A> AssetHandle<A>
where
    A: Asset,
{
    pub fn get_ready(&mut self) -> Option<AssetResult<A>> {
        self.get_impl(|| None)
    }

    pub fn get_impl(&mut self, waker: impl FnOnce() -> Option<Waker>) -> Option<AssetResult<A>> {
        loop {
            match &self.state {
                AssetHandleState::Asset { asset } => {
                    return Some(AssetResult {
                        path: self.path.clone(),
                        id: self.id.clone(),
                        state: AssetResultState::Asset(asset.clone()),
                    })
                }
                AssetHandleState::Error { error } => {
                    return Some(AssetResult {
                        path: self.path.clone(),
                        id: self.id.clone(),
                        state: AssetResultState::Error(error.clone()),
                    })
                }
                AssetHandleState::Searching {
                    path_shard,
                    key_hash,
                    shards,
                    random_state,
                } => {
                    let path = self
                        .path
                        .as_deref()
                        .expect("This state is only reachable when asset is requested with path");
                    let mut locked_shard = path_shard.lock();
                    let asset_entry = locked_shard
                        .raw_entry_mut()
                        .from_hash(*key_hash, |k| k.eq_key::<A>(path));

                    match asset_entry {
                        RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                            PathAssetState::Missing => {
                                drop(locked_shard);
                                self.state = AssetHandleState::Missing;
                                return Some(AssetResult {
                                    path: self.path.clone(),
                                    id: self.id.clone(),
                                    state: AssetResultState::Missing,
                                });
                            }
                            PathAssetState::Unloaded { wakers } => {
                                waker().map(|waker| wakers.push(waker));
                                return None;
                            }
                            PathAssetState::Loaded { id } => {
                                let id = *id;
                                self.id = Some(id);
                                let mut hasher = random_state.build_hasher();
                                hash_id_key::<A, _>(id, &mut hasher);
                                let key_hash = hasher.finish();

                                let shard = shards[key_hash as usize % shards.len()].clone();

                                drop(locked_shard);

                                self.state = AssetHandleState::Loading { key_hash, shard };
                            }
                        },
                        RawEntryMut::Vacant(_) => {
                            unreachable!()
                        }
                    }
                }
                AssetHandleState::Missing => {
                    return Some(AssetResult {
                        path: self.path.clone(),
                        id: self.id.clone(),
                        state: AssetResultState::Missing,
                    })
                }
                AssetHandleState::Loading { key_hash, shard } => {
                    let id = self
                        .id
                        .expect("This state can be reached only with known id");
                    let mut locked_shard = shard.lock();
                    let asset_entry = locked_shard
                        .raw_entry_mut()
                        .from_hash(*key_hash, |k| k.eq_key::<A>(id));

                    return match asset_entry {
                        RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                            AssetState::Error(err) => {
                                let err = err.clone();
                                drop(locked_shard);
                                self.state = AssetHandleState::Error { error: err.clone() };
                                Some(AssetResult {
                                    path: self.path.clone(),
                                    id: self.id.clone(),
                                    state: AssetResultState::Error(err),
                                })
                            }
                            AssetState::Missing => {
                                drop(locked_shard);
                                self.state = AssetHandleState::Missing;
                                Some(AssetResult {
                                    path: self.path.clone(),
                                    id: self.id.clone(),
                                    state: AssetResultState::Missing,
                                })
                            }
                            AssetState::Unloaded { wakers } => {
                                waker().map(|waker| wakers.push(waker));
                                None
                            }
                            AssetState::Typed(typed) => {
                                let typed: &StateTyped<A> = typed.downcast_ref().unwrap();
                                match typed {
                                    StateTyped::Asset { asset, .. } => {
                                        let asset = asset.clone();
                                        drop(locked_shard);
                                        self.state = AssetHandleState::Asset {
                                            asset: asset.clone(),
                                        };
                                        Some(AssetResult {
                                            path: self.path.clone(),
                                            id: self.id.clone(),
                                            state: AssetResultState::Asset(asset),
                                        })
                                    }
                                    StateTyped::Decoded { .. } => {
                                        drop(locked_shard);
                                        Some(AssetResult {
                                            path: self.path.clone(),
                                            id: self.id.clone(),
                                            state: AssetResultState::Decoded {
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

impl<A> Unpin for AssetHandle<A> {}

impl<A> Future for AssetHandle<A>
where
    A: Asset,
{
    type Output = AssetResult<A>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<AssetResult<A>> {
        let me = self.get_mut();

        match me.get_impl(|| Some(ctx.waker().clone())) {
            None => Poll::Pending,
            Some(result) => Poll::Ready(result),
        }
    }
}

pub struct AssetLookup {
    path_key: PathKey,
    state: AssetLookupState,
}

enum AssetLookupState {
    AssetId {
        id: AssetId,
    },
    Missing,
    Searching {
        key_hash: u64,
        path_shard: PathShard,
    },
}

impl AssetLookup {
    pub fn get_ready(&mut self) -> Option<Result<AssetId, NotFound>> {
        self.get_impl(|| None)
    }

    pub fn get_impl(
        &mut self,
        waker: impl FnOnce() -> Option<Waker>,
    ) -> Option<Result<AssetId, NotFound>> {
        loop {
            match &self.state {
                AssetLookupState::AssetId { id } => return Some(Ok(*id)),
                AssetLookupState::Searching {
                    path_shard,
                    key_hash,
                } => {
                    let mut locked_shard = path_shard.lock();
                    let asset_entry = locked_shard
                        .raw_entry_mut()
                        .from_key_hashed_nocheck(*key_hash, &self.path_key);

                    match asset_entry {
                        RawEntryMut::Occupied(mut entry) => match entry.get_mut() {
                            PathAssetState::Missing => {
                                drop(locked_shard);
                                self.state = AssetLookupState::Missing;
                                return Some(Err(NotFound {
                                    path: Some(self.path_key.path.clone()),
                                    id: None,
                                }));
                            }
                            PathAssetState::Unloaded { wakers } => {
                                waker().map(|waker| wakers.push(waker));
                                return None;
                            }
                            PathAssetState::Loaded { id } => {
                                let id = *id;
                                drop(locked_shard);

                                self.state = AssetLookupState::AssetId { id };
                            }
                        },
                        RawEntryMut::Vacant(_) => {
                            unreachable!()
                        }
                    }
                }
                AssetLookupState::Missing => {
                    return Some(Err(NotFound {
                        path: Some(self.path_key.path.clone()),
                        id: None,
                    }))
                }
            }
        }
    }
}

impl Unpin for AssetLookup {}

impl Future for AssetLookup {
    type Output = Result<AssetId, NotFound>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Result<AssetId, NotFound>> {
        let me = self.get_mut();

        match me.get_impl(|| Some(ctx.waker().clone())) {
            None => Poll::Pending,
            Some(result) => Poll::Ready(result),
        }
    }
}

impl Loader {
    /// Returns [`LoaderBuilder`] instance
    pub fn builder() -> LoaderBuilder {
        LoaderBuilder::new()
    }

    /// Lookups for the asset id by key string.
    pub fn lookup<A>(&self, path: &str) -> AssetLookup
    where
        A: Asset,
    {
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
                // Already queried. See status.

                let path_key = entry.key().clone();
                match entry.get() {
                    PathAssetState::Missing => AssetLookup {
                        path_key,
                        state: AssetLookupState::Missing,
                    },
                    PathAssetState::Unloaded { .. } => {
                        drop(locked_shard);

                        AssetLookup {
                            path_key,
                            state: AssetLookupState::Searching {
                                key_hash,
                                path_shard: path_shard.clone(),
                            },
                        }
                    }
                    PathAssetState::Loaded { id } => AssetLookup {
                        path_key,
                        state: AssetLookupState::AssetId { id: *id },
                    },
                }
            }
            RawEntryMut::Vacant(entry) => {
                let path_key = PathKey::new::<A>(path.into());
                let path = path_key.path.clone();

                // Register query
                let _ = entry.insert_hashed_nocheck(
                    key_hash,
                    path_key.clone(),
                    PathAssetState::Unloaded {
                        wakers: WakeOnDrop::new(),
                    },
                );
                drop(locked_shard);

                let loader = self.clone();
                let path_shard = path_shard.clone();

                let handle = AssetLookup {
                    path_key,
                    state: AssetLookupState::Searching {
                        key_hash,
                        path_shard: path_shard.clone(),
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
                        // Already queried. See status.

                        let path_key = entry.key().clone();
                        match entry.get() {
                            PathAssetState::Missing => AssetHandle {
                                path: Some(path_key.path.clone()),
                                id: None,
                                state: AssetHandleState::Missing,
                            },
                            PathAssetState::Unloaded { .. } => {
                                drop(locked_shard);

                                AssetHandle {
                                    path: Some(path_key.path),
                                    id: None,
                                    state: AssetHandleState::Searching {
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
                                    path: Some(path_key.path.clone()),
                                    id: Some(id),
                                    state: AssetHandleState::Loading {
                                        key_hash,
                                        shard: shard.clone(),
                                    },
                                }
                            }
                        }
                    }
                    RawEntryMut::Vacant(entry) => {
                        let path_key = PathKey::new::<A>(path.into());
                        let path = path_key.path.clone();

                        // Register query
                        let _ = entry.insert_hashed_nocheck(
                            key_hash,
                            path_key.clone(),
                            PathAssetState::Unloaded {
                                wakers: WakeOnDrop::new(),
                            },
                        );
                        drop(locked_shard);

                        let loader = self.clone();
                        let path_shard = path_shard.clone();

                        let handle = AssetHandle {
                            path: Some(path_key.path),
                            id: None,
                            state: AssetHandleState::Searching {
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

                match asset_entry {
                    RawEntryMut::Occupied(entry) => {
                        match entry.get() {
                            // Already queried. See status.
                            AssetState::Error(err) => AssetHandle {
                                path: None,
                                id: Some(id),
                                state: AssetHandleState::Error { error: err.clone() },
                            },
                            AssetState::Missing => AssetHandle {
                                path: None,
                                id: Some(id),
                                state: AssetHandleState::Missing,
                            },
                            AssetState::Unloaded { .. } => AssetHandle {
                                path: None,
                                id: Some(id),
                                state: AssetHandleState::Loading {
                                    key_hash,
                                    shard: shard.clone(),
                                },
                            },
                            AssetState::Typed(typed) => {
                                let typed: &StateTyped<A> =
                                    <dyn Any>::downcast_ref(&**typed).unwrap();
                                match typed {
                                    StateTyped::Asset { asset, .. } => AssetHandle {
                                        path: None,
                                        id: Some(id),
                                        state: AssetHandleState::Asset {
                                            asset: asset.clone(),
                                        },
                                    },
                                    StateTyped::Decoded { .. } => AssetHandle {
                                        path: None,
                                        id: Some(id),
                                        state: AssetHandleState::Loading {
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
                            path: None,
                            id: Some(id),
                            state: AssetHandleState::Loading {
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
                        lock: Arc::new(spin::Mutex::new(DecodedState {
                            decoded: Some(decoded),
                            version: data.version,
                            source: data.source,
                        })),
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
