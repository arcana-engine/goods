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
mod formats;
mod handle;
mod loader;
mod process;
mod queue;
mod registry;
mod source;
mod sync;

#[cfg(feature = "legion")]
mod legion;

pub use self::{asset::*, formats::*, handle::*, loader::*, registry::*, registry::*, source::*};

use {
    crate::{
        process::AnyProcesses,
        queue::Queue,
        sync::{Lock, Ptr, WeakPtr},
    },
    alloc::boxed::Box,
    core::{
        any::TypeId,
        fmt::{self, Debug, Display},
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

impl<A> Clone for Error<A>
where
    A: Asset,
{
    fn clone(&self) -> Self {
        match self {
            Error::NotFound => Error::NotFound,
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
    pub(crate) inner: Ptr<Inner<K>>,
}

struct Inner<K> {
    cache: Lock<HashMap<(TypeId, K), AnyHandle>>,
    processes: Lock<HashMap<TypeId, AnyProcesses<K>>>,
    pub(crate) loader: Queue<LoaderTask<K>>,
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
    pub fn new(registry: Registry<K>) -> Self {
        Cache {
            registry,
            inner: Ptr::new(Inner {
                cache: Lock::default(),
                processes: Lock::default(),
                loader: Queue::new(),
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
                let (handle, slot) = self
                    .inner
                    .processes
                    .lock()
                    .entry(TypeId::of::<A::Context>())
                    .or_insert_with(|| AnyProcesses::new::<A::Context>())
                    .alloc();

                entry.insert(handle.clone().into());
                drop(lock);

                self.inner.loader.push(LoaderTask::new(
                    Box::pin(self.registry.clone().read(key)),
                    format,
                    slot,
                ));
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
        let mut lock = self.inner.processes.lock();
        if let Some(processes) = lock.get_mut(&TypeId::of::<C>()) {
            let processes = processes.run();
            drop(lock);
            for process in processes {
                process.run(ctx);
            }
        }
    }

    /// Process all `SimpleAsset`s.
    pub fn process_simple(&self)
    where
        K: 'static,
    {
        self.process(&mut PhantomContext);
    }

    /// Create loader for this `Cache`.
    /// `Loader` will drive async loading tasks to completion
    /// and resolves only after `Cache` is destroyed (all clones are dropped).
    /// To await only pending tasks use `Loader::flush` method.
    pub fn loader(&self) -> Loader<K> {
        Loader::new(self)
    }

    fn downgrade(&self) -> WeakCache<K> {
        WeakCache {
            registry: self.registry.clone(),
            inner: Ptr::downgrade(&self.inner),
        }
    }
}

struct WeakCache<K> {
    registry: Registry<K>,
    inner: WeakPtr<Inner<K>>,
}

impl<K> Clone for WeakCache<K> {
    fn clone(&self) -> Self {
        WeakCache {
            registry: self.registry.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<K> WeakCache<K> {
    fn upgrade(&self) -> Option<Cache<K>> {
        let inner = self.inner.upgrade()?;

        Cache {
            registry: self.registry.clone(),
            inner,
        }
        .into()
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
