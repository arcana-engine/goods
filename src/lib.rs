//! # Goods
//!
//! Easy-to-use asset manager for many environments.
//!
//! ## Goals
//!
//! This crate is written with following goals in mind:
//!
//! * **Batteries included.**\
//!   Crate comes with variety of simple data sources like `FileSource` and `HttpSource`.
//!   Few `Format`s based on serde are included under feature flags.
//!
//! * **Extensibility.**\
//!   Multiple `Format` traits can be implemented for any asset type, including foreign asset types.\
//!   For example `JsonFormat`, `YamlFormat` and `RonFormat` (bundled in the crate) implement `Format` trait for any asset type
//!   which intermediate representation implements `serde::de::DeserializeOwned`.
//!
//! * **Supporting WebAssembly.**\
//!   All mandatory dependencies are WASM-compatible and no threading is required for asset loading to work.
//!
//! * **Working with asynchronous data sources.**\
//!   All data sources implement `Source` trait.
//!   `Source::read` method returns future that will be driven to completion by the bound `Loader`.
//!
//! * **no_std**\
//!     `alloc` is required.
//!
//! * **Fast compilation.**\
//!     build after `cargo clean` takes ~3s.
//!
//! ## Non-Goals
//!
//! This crate is not aimed to support every possible feature.
//! Here's list of some of those features:
//!
//! * Hot-reloading\
//!   Currently there are no plans to support hot-reloading.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(doc, feature(doc_cfg))]

extern crate alloc;

mod asset;
mod channel;
mod formats;
mod handle;
mod process;
mod registry;
mod source;
mod spawn;
mod sync;

#[cfg(feature = "legion")]
mod legion;

pub use self::{asset::*, formats::*, handle::*, registry::*, registry::*, source::*, spawn::*};

use {
    crate::{
        channel::{slot, Sender},
        process::{AnyProcess, Process, Processor},
        sync::{BoxFuture, Lock, Ptr, Send, Sync},
    },
    alloc::{boxed::Box, vec::Vec},
    core::{
        any::TypeId,
        fmt::{self, Debug, Display},
        future::Future,
        hash::Hash,
    },
    hashbrown::hash_map::{Entry, HashMap},
};

#[cfg(not(feature = "sync"))]
use alloc::rc::Rc;

#[cfg(feature = "sync")]
use alloc::sync::Arc;

/// Error occured in process of asset loading.
pub enum Error<A: Asset> {
    /// Asset was not found among registered sources.
    NotFound,

    /// Failed to spawn loading task.
    SpawnError,

    /// Asset instance decoding or building failed.
    ///
    /// Specifically this error may occur in `Asset::build` and `Format::decode`.
    #[cfg(not(feature = "sync"))]
    Asset(Rc<A::Error>),

    /// Asset instance decoding or building failed.
    ///
    /// Specifically this error may occur in `Asset::build` and `Format::decode`.
    #[cfg(feature = "sync")]
    Asset(Arc<A::Error>),

    /// Source in which asset was found failed to load it.
    #[cfg(all(not(feature = "std"), not(feature = "sync")))]
    Source(Rc<dyn Display>),

    /// Source in which asset was found failed to load it.
    #[cfg(all(not(feature = "std"), feature = "sync"))]
    Source(Arc<dyn Display + Send + Sync>),

    /// Source in which asset was found failed to load it.
    #[cfg(all(feature = "std", not(feature = "sync")))]
    Source(Rc<dyn std::error::Error>),

    /// Source in which asset was found failed to load it.
    #[cfg(all(feature = "std", feature = "sync"))]
    Source(Arc<dyn std::error::Error + Send + Sync>),
}

impl<A> From<SourceError> for Error<A>
where
    A: Asset,
{
    fn from(err: SourceError) -> Self {
        match err {
            SourceError::NotFound => Error::NotFound,
            SourceError::Error(err) => Error::Source(err),
        }
    }
}

impl<A> Clone for Error<A>
where
    A: Asset,
{
    fn clone(&self) -> Self {
        match self {
            Error::NotFound => Error::NotFound,
            Error::SpawnError => Error::SpawnError,
            Error::Asset(err) => Error::Asset(err.clone()),
            Error::Source(err) => Error::Source(err.clone()),
        }
    }
}

impl<A> Debug for Error<A>
where
    A: Asset,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => fmt.write_str("Error::NotFound"),
            Error::SpawnError => fmt.write_str("Error::SpawnError"),
            Error::Asset(err) => write!(fmt, "Error::Asset({})", err),
            Error::Source(err) => write!(fmt, "Error::Source({})", err),
        }
    }
}

impl<A> Display for Error<A>
where
    A: Asset,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => fmt.write_str("Asset not found"),
            Error::SpawnError => fmt.write_str("Failed to spawn loading task"),
            Error::Asset(err) => write!(fmt, "Asset error: {}", err),
            Error::Source(err) => write!(fmt, "Source error: {}", err),
        }
    }
}

#[cfg(feature = "std")]
impl<A> std::error::Error for Error<A>
where
    A: Asset,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NotFound => None,
            Error::SpawnError => None,
            Error::Asset(err) => Some(&**err),
            Error::Source(err) => Some(&**err),
        }
    }
}

/// Asset cache.
/// This type is main entry point for asset loading.
/// Caches loaded assets and provokes loading work for new assets.
pub struct Cache<K> {
    registry: Registry<K>,
    #[cfg(not(feature = "sync"))]
    inner: Ptr<Inner<K, dyn Spawn>>,

    #[cfg(feature = "sync")]
    inner: Ptr<Inner<K, dyn Spawn + Send + Sync>>,
}

struct Inner<K, S: ?Sized> {
    cache: Lock<HashMap<(TypeId, K), AnyHandle>>,
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
        Cache {
            registry,
            inner: Ptr::new(Inner {
                cache: Lock::default(),
                processor: Processor::new(),
                spawn,
            }),
        }
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses default asset format for decoding.
    pub fn load<A>(&self, key: K) -> Handle<A>
    where
        K: Eq + Hash + Clone + Send + Sync + 'static,
        A: AssetDefaultFormat<K>,
    {
        self.load_with_format(key, A::DefaultFormat::default())
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses provided asset format for decoding.
    pub fn load_with_format<A, F>(&self, key: K, format: F) -> Handle<A>
    where
        K: Eq + Hash + Clone + Send + Sync + 'static,
        A: Asset,
        F: Format<A, K>,
    {
        let tid = TypeId::of::<A>();

        let mut lock = self.inner.cache.lock();

        match lock.entry((tid, key.clone())) {
            Entry::Occupied(entry) => {
                let any = entry.get().clone();
                drop(lock);
                any.downcast::<A>().unwrap()
            }
            Entry::Vacant(entry) => {
                let handle = Handle::new();
                entry.insert(handle.clone().into());
                drop(lock);

                let task: BoxFuture<'_, _> =
                    if TypeId::of::<A::Context>() == TypeId::of::<PhantomContext>() {
                        Box::pin(load_asset_with_phantom_context(
                            self.registry.clone().read(key),
                            format,
                            self.clone(),
                            handle.clone(),
                        ))
                    } else {
                        Box::pin(load_asset(
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

                handle
            }
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

#[cfg(feature = "sync")]
#[allow(dead_code)]
fn test_loader_send_sync<K: Send>() {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}

    is_send::<Cache<K>>();
    is_sync::<Cache<K>>();
}

pub(crate) async fn load_asset<A, F, K, L>(
    loading: L,
    format: F,
    cache: Cache<K>,
    process_sender: Sender<Box<dyn AnyProcess<A::Context>>>,
    handle: Handle<A>,
) where
    A: Asset,
    F: Format<A, K>,
    L: Future<Output = Result<Vec<u8>, SourceError>> + Send + 'static,
{
    handle.set(
        async move {
            let bytes = loading.await?;
            let decode = format.decode(bytes, &cache);
            drop(cache);
            let repr = decode.await.map_err(|err| Error::Asset(Ptr::new(err)))?;
            let (slot, setter) = slot::<A::BuildFuture>();
            process_sender.send(Box::new(Process::<A> { repr, setter }));
            slot.await.await.map_err(|err| Error::Asset(Ptr::new(err)))
        }
        .await,
    )
}

pub(crate) async fn load_asset_with_phantom_context<A, F, K, L>(
    loading: L,
    format: F,
    cache: Cache<K>,
    handle: Handle<A>,
) where
    A: Asset,
    F: Format<A, K>,
    L: Future<Output = Result<Vec<u8>, SourceError>> + Send + 'static,
{
    debug_assert_eq!(TypeId::of::<A::Context>(), TypeId::of::<PhantomContext>());

    handle.set(
        async move {
            let bytes = loading.await?;
            let decode = format.decode(bytes, &cache);
            drop(cache);
            let repr = decode.await.map_err(|err| Error::Asset(Ptr::new(err)))?;
            let build = A::build(repr, unsafe {
                &mut *{ &mut PhantomContext as *mut _ as *mut A::Context }
            });
            build.await.map_err(|err| Error::Asset(Ptr::new(err)))
        }
        .await,
    )
}
