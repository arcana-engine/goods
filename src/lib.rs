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
mod cache;
mod channel;
mod error;
mod formats;
mod handle;
mod key;
mod process;
mod registry;
mod source;
mod spawn;

pub use self::{
    asset::*, cache::*, error::*, formats::*, handle::*, key::*, registry::*, source::*, spawn::*,
};

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
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
pub const fn ready<T>(value: T) -> Ready<T> {
    Ready(Some(value))
}
