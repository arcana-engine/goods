
<img src="logo/goods.logo.svg" width="200" />

# goods

Easy-to-use asset manager for many environments.

[![crates](https://img.shields.io/crates/v/goods.svg?label=goods)](https://crates.io/crates/goods)
[![docs](https://docs.rs/goods/badge.svg)](https://docs.rs/goods)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![License](https://img.shields.io/badge/license-APACHE-blue.svg)](LICENSE-APACHE)

Easy-to-use asset manager for many environments.

## Goals

This crate is written with following goals in mind:

* **Batteries included.**\
  Crate comes with variety of simple data sources like [`FileSource`] and [`HttpSource`].
  Few [`Format`]s based on serde are included under feature flags.\
  More [`Source`]s and [`Format`]s can be added.

* **Extensibility.**\
  Multiple [`Format`] traits can be implemented for any asset type, including foreign asset types.\
  For example [`JsonFormat`], [`YamlFormat`] and [`RonFormat`] (bundled in the crate) implement [`Format`] trait for any asset type
  which intermediate representation implements [`serde::de::DeserializeOwned`].

* **Supporting WebAssembly.**\
  All mandatory dependencies are WASM-compatible and no threading is required for asset loading to work.

* **Working with asynchronous data sources.**\
  All data sources must implement [`Source`] trait.
  [`Source::read`] method returns future that will be driven to completion by the bound executor - see [`Spawn`].

* **no_std**\
    But [`alloc`] is required.

* **Fast compilation.**\
    build after `cargo clean` takes ~3s.

## Non-Goals

This crate is not aimed to support every possible feature.
Here's list of some of those features:

* Hot-reloading\
   Currently there are no plans to support hot-reloading.

## Features

All out-of-the-box functionality exept core traits and types can be enabled with features.

### General

* `std` - adds implementation of [`std::error::Error`] trait for error types.
  Enabled by default.
* `sync` - makes most types [`Send`] and some [`Sync`]. Adds requirements for traits implementations to be [`Send`] and [`Sync`] where needed.
  Enabled by default.

### Sources

* `fs` (enables `std`) - adds [`FileSource`] - [`Source`] implementation that loads asset bytes from file-system.
* `reqwest` - adds [`ReqwestSource`] - [`Source`] implementation that loads asset bytes from URLs using [`reqwest`].
  Using this source requires spawner to spawn tasks with [`tokio`]. Otherwise [`reqwest`] interals will panic.
* `fetch` - adds [`FetchSource`] that uses browser's Fetch API to load assets data. *Conflicts with `sync` feature*.

### Formats

* `json-format` - adds [`JsonFormat`] - [`Format`] implementation that treats asset bytes as JSON document and deserializes asset representation via serde
* `yaml-format` - adds [`YamlFormat`] - [`Format`] implementation that treats asset bytes as YAML document and deserializes asset representation via serde
* `ron-format` - adds [`RonFormat`] - [`Format`] implementation that treats asset bytes as RON document and deserializes asset representation via serde

### Spawners

* `futures-spawn` - adds [`Spawn`] implementation for [`futures_task::Spawn`](aka [`futures::task::Spawn`]) allowing to use compatible spawners to drive loading tasks to completion.
* `wasm-bindgen-spawn` - adds [`Spawn`] implementations that uses [`wasm_bindgen_futures::spawn_local`] to drive loadin tasks. Usable only on `wasm32` target.
* `tokio-spawn` - adds [`Spawn`] implementation for [`goods::Tokio`]([`tokio::runtime::Handle`] wrapper) allowing tokio to drive loading tasks. [`ReqwestSource`] requires [`tokio`] runtime.

## Examples

There are few simple examples provided already.

### [fs examlple](./examples/fs.rs)
Shows how to build registry with [`FileSource`] and load simple assets from it.

### [reqwest example](./examples/reqwest.rs)
Async example that loads assets using HTTP protocol with [`tokio`] and [`reqwest`] crates.

### [legion example](./examples/legion.rs)
Shows how to load assets directly into entity using [`legion`] ECS crate.

### [fetch example](./examples/fetch.rs)
Shows how to load assets in browser using Fetch API.

This example can be built using [build-wasm32.sh](./examples/build-wasm32.sh) or [build-wasm32.bat](./examples/build-wasm32.bat) in [examples](./examples) directory.\
`wasm-bindgen` (compatible version) and `wasm-opt` must be in `PATH`

```sh
cd examples
build-wasm32 fetch --features std,fetch,json-format,yaml-format,wasm-bindgen-spawn
python3 server.py
```

Then open http://localhost:8000/fetch in your favorite browser.
Loaded assets must be shown on the page. Otherwise see for errors in log.

## Gotchas

* Currently asyn/await doesn't work with `no_std` on `stable`
* `sync` conflicts with `fetch` feature. But in general `sync` is not necessary when targeting web browser.

## License

This repository is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution Licensing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[`alloc`]: https://doc.rust-lang.org/alloc/index.html
[`HttpSource`]: https://docs.rs/goods/latest/goods/struct.HttpSource.html
[`serde::de::DeserializeOwned`]: https://docs.rs/serde/1/serde/de/trait.DeserializeOwned.html
[`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html
[`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`FileSource`]: https://docs.rs/goods/latest/goods/struct.FileSource.html
[`Source`]: https://docs.rs/goods/latest/goods/trait.Source.html
[`Source::read`]: https://docs.rs/goods/latest/goods/trait.Source.html#tymethod.read
[`ReqwestSource`]: https://docs.rs/goods/latest/goods/struct.ReqwestSource.html
[`FetchSource`]: https://docs.rs/goods/latest/wasm32-unknown-unknown/goods/struct.FetchSource.html
[`tokio`]: https://docs.rs/tokio
[`reqwest`]: https://docs.rs/reqwest
[`legion`]: https://docs.rs/legion
[`Format`]: https://docs.rs/goods/latest/goods/trait.Format.html
[`JsonFormat`]: https://docs.rs/goods/latest/goods/struct.JsonFormat.html
[`YamlFormat`]: https://docs.rs/goods/latest/goods/struct.YamlFormat.html
[`RonFormat`]: https://docs.rs/goods/latest/goods/struct.RonFormat.html
[`Spawn`]: https://docs.rs/goods/latest/goods/trait.Spawn.html
[`futures_task::Spawn`]: https://docs.rs/futures-task/0.3/futures_task/trait.Spawn.html
[`futures::task::Spawn`]: https://docs.rs/futures/0.3/futures/task/trait.Spawn.html
[`wasm_bindgen_futures::spawn_local`]: https://docs.rs/wasm-bindgen-futures/0.4/wasm_bindgen_futures/fn.spawn_local.html
[`goods::Tokio`]: ./src/spawn.rs#L21
[`tokio::runtime::Handle`]: https://docs.rs/tokio/0.2/tokio/runtime/struct.Handle.html
