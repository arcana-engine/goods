//!
//! Easy-to-use asset manager for many environments.
//!
//! # Goals
//!
//! This crate is written with following goals in mind:
//!
//! * **Batteries included.**\
//!   Crate comes with variety of simple data sources like [`FileSource`] and [`HttpSource`].
//!   Few [`Format`]s based on serde are included under feature flags.\
//!   More [`Source`]s and [`Format`]s can be added.
//!
//! * **Extensibility.**\
//!   Multiple [`Format`] traits can be implemented for any asset type, including foreign asset types.\
//!   For example [`JsonFormat`], [`YamlFormat`] and [`RonFormat`] (bundled in the crate) implement [`Format`] trait for any asset type
//!   which intermediate representation implements [`serde::de::DeserializeOwned`].
//!
//! * **Supporting WebAssembly.**\
//!   All mandatory dependencies are WASM-compatible and no threading is required for asset loading to work.
//!
//! * **Working with asynchronous data sources.**\
//!   All data sources must implement [`Source`] trait.
//!   [`Source::read`] method returns future that will be driven to completion by the bound executor - see [`Spawn`].
//!
//! * **no_std**\
//!     But [`alloc`] is required.
//!
//! * **Fast compilation.**\
//!     build after `cargo clean` takes ~3s.
//!
//! # Non-Goals
//!
//! This crate is not aimed to support every possible feature.
//! Here's list of some of those features:
//!
//! * Hot-reloading\
//!    Currently there are no plans to support hot-reloading.
//!
//! # Features
//!
//! All out-of-the-box functionality exept core traits and types can be enabled with features.
//!
//! ## General
//!
//! * `std` - adds implementation of [`std::error::Error`] trait for error types.
//!   Enabled by default.
//! * `sync` - makes most types [`MaybeSend`] and some [`MaybeSync`]. Adds requirements for traits implementations to be [`MaybeSend`] and [`MaybeSync`] where needed.
//!   Enabled by default.
//!
//! ## Sources
//!
//! * `fs` (enables `std`) - adds [`FileSource`] - [`Source`] implementation that loads asset bytes from file-system.
//! * `reqwest` - adds [`ReqwestSource`] - [`Source`] implementation that loads asset bytes from URLs using [`reqwest`].
//!   Using this source requires spawner to spawn tasks with [`tokio`]. Otherwise [`reqwest`] interals will panic.
//! * `fetch` - adds [`FetchSource`] that uses browser's Fetch API to load assets data. *Conflicts with `sync` feature*.
//!
//! ## Formats
//!
//! * `json-format` - adds [`JsonFormat`] - [`Format`] implementation that treats asset bytes as JSON document and deserializes asset representation via serde
//! * `yaml-format` - adds [`YamlFormat`] - [`Format`] implementation that treats asset bytes as YAML document and deserializes asset representation via serde
//! * `ron-format` - adds [`RonFormat`] - [`Format`] implementation that treats asset bytes as RON document and deserializes asset representation via serde
//!
//! ## Spawners
//!
//! * `futures-spawn` - adds [`Spawn`] implementation for [`futures_task::Spawn`](aka [`futures::task::Spawn`]) allowing to use compatible spawners to drive loading tasks to completion.
//! * `wasm-bindgen-spawn` - adds [`Spawn`] implementations that uses [`wasm_bindgen_futures::spawn_local`] to drive loadin tasks. Usable only on `wasm32` target.
//! * `tokio-spawn` - adds [`Spawn`] implementation for [`goods::Tokio`]([`tokio::runtime::Handle`] wrapper) allowing tokio to drive loading tasks. [`ReqwestSource`] requires [`tokio`] runtime.
//!
//! [`alloc`]: https://doc.rust-lang.org/alloc/index.html
//! [`HttpSource`]: ./struct.HttpSource.html
//! [`serde::de::DeserializeOwned`]: https://docs.rs/serde/1/serde/de/trait.DeserializeOwned.html
//! [`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html
//! [`MaybeSend`]: https://doc.rust-lang.org/std/marker/trait.MaybeSend.html
//! [`MaybeSync`]: https://doc.rust-lang.org/std/marker/trait.MaybeSync.html
//! [`FileSource`]: ./struct.FileSource.html
//! [`Source`]: ./trait.Source.html
//! [`Source::read`]: ./trait.Source.html#tymethod.read
//! [`ReqwestSource`]: ./struct.ReqwestSource.html
//! [`FetchSource`]: https://docs.rs/goods/latest/wasm32-unknown-unknown/goods/struct.FetchSource.html
//! [`tokio`]: https://docs.rs/tokio
//! [`reqwest`]: https://docs.rs/reqwest
//! [`Format`]: ./trait.Format.html
//! [`JsonFormat`]: ./struct.JsonFormat.html
//! [`YamlFormat`]: ./struct.YamlFormat.html
//! [`RonFormat`]: ./struct.RonFormat.html
//! [`Spawn`]: ./trait.Spawn.html
//! [`futures_task::Spawn`]: https://docs.rs/futures-task/0.3/futures_task/trait.Spawn.html
//! [`futures::task::Spawn`]: https://docs.rs/futures/0.3/futures/task/trait.Spawn.html
//! [`wasm_bindgen_futures::spawn_local`]: https://docs.rs/wasm-bindgen-futures/0.4/wasm_bindgen_futures/fn.spawn_local.html
//! [`goods::Tokio`]: ./src/spawn.rs#L21
//! [`tokio::runtime::Handle`]: https://docs.rs/tokio/0.2/tokio/runtime/struct.Handle.html

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(all(doc, feature = "unstable-doc"), feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/zakarumych/goods/master/logo/goods.logo.png"
)]

extern crate alloc;

mod asset;
mod bytes;
mod channel;
mod formats;
mod handle;
mod process;
mod registry;
mod source;
mod spawn;

pub use self::{asset::*, formats::*, handle::*, registry::*, registry::*, source::*, spawn::*};

use {
    crate::{
        channel::{slot, Sender},
        process::{AnyProcess, Process, Processor},
    },
    alloc::{boxed::Box, vec::Vec},
    core::{
        any::TypeId,
        fmt::{self, Debug, Display},
        future::Future,
        hash::Hash,
        pin::Pin,
        task::{Context, Poll},
    },
    hashbrown::hash_map::{Entry, HashMap},
    maybe_sync::{dyn_maybe_send_sync, BoxFuture, MaybeSend, MaybeSync, Mutex, Rc},
};

/// Immediatelly ready future.
/// Can be used for [`Format::DecodeFuture`]
/// when format doesn't need to await anything.
///
/// [`Format::DecodeFuture`]: ./trait.Format.html#associatedtype.DecodeFuture
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Ready<T>(Option<T>);

impl<T> Unpin for Ready<T> {}

impl<T> Future for Ready<T> {
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _ctx: &mut Context<'_>) -> Poll<T> {
        Poll::Ready(self.0.take().expect("Ready polled after completion"))
    }
}

/// Creates immediately ready future.
pub fn ready<T>(value: T) -> Ready<T> {
    Ready(Some(value))
}

/// Error occured in process of asset loading.
pub enum Error<A: Asset> {
    /// Asset was not found among registered sources.
    NotFound,

    /// Failed to spawn loading task.
    SpawnError,

    /// Asset instance building failed.
    ///
    /// Specifically this error may occur in [`Asset::build`].
    ///
    /// [`Asset::build`]: ./trait.Asset.html#tymethod.build
    Asset(Rc<A::Error>),

    /// Asset decoding failed.
    ///
    /// Specifically this error may occur in [`Format::decode`].
    ///
    /// [`Format::decode`]: ./trait.Format.html#tymethod.decode
    #[cfg(not(feature = "std"))]
    Format(Rc<dyn_maybe_send_sync!(Display)>),

    /// Asset decoding failed.
    ///
    /// Specifically this error may occur in [`Format::decode`].
    ///
    /// [`Format::decode`]: ./trait.Format.html#tymethod.decode
    #[cfg(feature = "std")]
    Format(Rc<dyn_maybe_send_sync!(std::error::Error)>),
    /// Source in which asset was found failed to load it.
    #[cfg(not(feature = "std"))]
    Source(Rc<dyn_maybe_send_sync!(Display)>),

    /// Source in which asset was found failed to load it.
    #[cfg(feature = "std")]
    Source(Rc<dyn_maybe_send_sync!(std::error::Error)>),
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
            Error::Format(err) => Error::Format(err.clone()),
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
            Error::Format(err) => write!(fmt, "Error::Format({})", err),
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
            Error::Format(err) => write!(fmt, "Format error: {}", err),
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
            Error::Format(err) => Some(&**err),
            Error::Source(err) => Some(&**err),
        }
    }
}

pub trait Key: Eq + Hash + Clone + MaybeSend + MaybeSync + 'static {}

impl<T> Key for T where T: Eq + Hash + Clone + MaybeSend + MaybeSync + 'static {}

/// Asset cache.
/// This type is main entry point for asset loading.
/// Caches loaded assets and provokes loading work for new assets.
pub struct Cache<K> {
    registry: Registry<K>,
    #[cfg(not(feature = "sync"))]
    inner: Rc<Inner<K, dyn Spawn>>,

    #[cfg(feature = "sync")]
    inner: Rc<Inner<K, dyn Spawn + MaybeSend + MaybeSync>>,
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
        S: Spawn + MaybeSend + MaybeSync + 'static,
    {
        Cache {
            registry,
            inner: Rc::new(Inner {
                cache: Mutex::default(),
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
        K: Key,
        A: AssetDefaultFormat<K>,
    {
        self.load_with_format(key, A::DefaultFormat::default())
    }

    /// Requests an asset by the `key`.
    /// Returns cached asset handle if same asset type was loaded from same `key` (even if loading is incomplete).
    /// Uses provided asset format for decoding.
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
fn test_loader_send_sync<K: MaybeSend>() {
    fn is_send<T: MaybeSend>() {}
    fn is_sync<T: MaybeSync>() {}

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
    L: Future<Output = Result<Vec<u8>, SourceError>> + MaybeSend + 'static,
{
    handle.set(
        async move {
            let bytes = loading.await?;
            let decode = format.decode(bytes, &cache);
            drop(cache);
            let repr = decode.await.map_err(|err| Error::Format(Rc::new(err)))?;
            let (slot, setter) = slot::<A::BuildFuture>();
            process_sender.send(Box::new(Process::<A> { repr, setter }));
            slot.await.await.map_err(|err| Error::Asset(Rc::new(err)))
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
    L: Future<Output = Result<Vec<u8>, SourceError>> + MaybeSend + 'static,
{
    debug_assert_eq!(TypeId::of::<A::Context>(), TypeId::of::<PhantomContext>());

    handle.set(
        async move {
            let bytes = loading.await?;
            let decode = format.decode(bytes, &cache);
            drop(cache);
            let repr = decode.await.map_err(|err| Error::Format(Rc::new(err)))?;
            let build = A::build(repr, unsafe {
                &mut *{ &mut PhantomContext as *mut _ as *mut A::Context }
            });
            build.await.map_err(|err| Error::Asset(Rc::new(err)))
        }
        .await,
    )
}
