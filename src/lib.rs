//!
//! # Goods - asset manager
//! Loads only good assets.
//!
//! ## Goals
//! * **Batteries included.**\
//!   Crate comes with variety of simple data sources like `FileSource` and `HttpSource`.
//!   Few `Format`s based on serde are included under feature flags.
//!
//! * **Extensible.**\
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod asset;
mod channel;
mod formats;
mod handle;
mod loader;
mod process;
mod registry;
mod source;

#[cfg(feature = "legion")]
mod legion;

pub use self::{asset::*, formats::*, handle::*, loader::*, registry::*, registry::*, source::*};

use {
    crate::{channel::Sender, process::AnyProcesses},
    alloc::{boxed::Box, sync::Arc},
    core::{
        any::TypeId,
        fmt::{self, Debug, Display},
        hash::Hash,
    },
    hashbrown::hash_map::{Entry, HashMap},
    spin::Mutex,
};

/// Error occured in process of asset loading.
pub enum Error<A: Asset> {
    /// Asset was not found among registered sources.
    NotFound,

    /// Asset instance decoding or building failed.
    ///
    /// Specifically this error may occur in `Asset::build` and `Format::decode`.
    Asset(Arc<A::Error>),

    /// Source in which asset was found failed to load it.
    #[cfg(feature = "std")]
    Source(Arc<dyn std::error::Error + Send + Sync>),

    /// Source in which asset was found failed to load it.
    #[cfg(not(feature = "std"))]
    Source(Arc<dyn core::fmt::Display + Send + Sync>),
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
    cache: Mutex<HashMap<(TypeId, K), AnyHandle>>,
    processes: Mutex<HashMap<TypeId, AnyProcesses<K>>>,
    loader_sender: Sender<LoaderTask<K>>,
}

impl<K> Cache<K> {
    /// Creates new asset cache.
    /// Assets will be loaded from provided `registry`.
    /// Loading tasks will be sent to `Loader`.
    pub fn new(registry: Registry<K>, loader: &Loader<K>) -> Self {
        Cache {
            registry,
            cache: Mutex::default(),
            processes: Mutex::default(),
            loader_sender: loader.sender(),
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

        let mut lock = self.cache.lock();

        match lock.entry((tid, key.clone())) {
            Entry::Occupied(entry) => {
                let any = entry.get().clone();
                drop(lock);
                any.downcast::<A>().unwrap()
            }
            Entry::Vacant(entry) => {
                let (handle, slot) = self
                    .processes
                    .lock()
                    .entry(TypeId::of::<A::Context>())
                    .or_insert_with(|| AnyProcesses::new::<A::Context>())
                    .alloc();

                entry.insert(handle.clone().into());
                drop(lock);

                self.loader_sender.send(LoaderTask::new(
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
        let mut lock = self.processes.lock();
        if let Some(processes) = lock.get_mut(&TypeId::of::<C>()) {
            let processes = processes.run();
            drop(lock);
            for process in processes {
                process.run(ctx);
            }
        }
    }

    /// Process all `SimpleAsset` types.
    pub fn process_simple(&self)
    where
        K: 'static,
    {
        self.process(&mut PhantomContext);
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn test_loader_send_sync<K: Send>() {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}

    is_send::<Cache<K>>();
    is_sync::<Cache<K>>();
}
