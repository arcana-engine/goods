
<img src="logo/goods.logo.svg" width="200" />

# goods

Easy-to-use asset manager for many environments.

[![crates](https://img.shields.io/crates/v/goods.svg?style=for-the-badge&label=goods)](https://crates.io/crates/goods)
[![docs](https://img.shields.io/badge/docs.rs-goods-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/goods)
[![actions](https://img.shields.io/github/workflow/status/zakarumych/goods/Rust/master?style=for-the-badge)](https://github.com/zakarumych/goods/actions?query=workflow%3ARust)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/zakarumych/goods?style=for-the-badge)

Easy-to-use asset manager for many environments.

## Goals

This crate is written with following goals in mind:

* **Batteries included.**\
  Extension crates provide variety of useful data sources like [`FileSource`] and [`ReqwestSource`].
  Serde based [`Format`]s are provided by [`goods-json`], [`goods-yaml`] and [`goods-ron`].

* **Extensibility.**\
  Multiple [`Format`] traits can be implemented for any asset type, including foreign asset types.\
  For example [`JsonFormat`], [`YamlFormat`] and [`RonFormat`] implement [`Format`] trait for any asset type
  which intermediate representation implements [`serde::de::DeserializeOwned`].

* **Supporting WebAssembly.**\
  This crate and some of the extension crates are WASM-compatible and no threading is required for asset loading to work.
  Types and traits prefixed with `Local` remote [`Send`] and [`Sync`] from requirements and bounds.
  They can be used in single-threaded environment. Added specifically for WASM where `!Send` and `!Sync` types are common.

* **Working with asynchronous data sources.**\
  Raw data sources implement [`Source`] trait.
  [`Source::read`] method returns future that will be driven to completion by polling handle to asset.

* **Fast compilation.**\
  core crate ([`goods`]) build after `cargo clean` takes ~1s.

## Non-Goals

This crate is not aimed to support every possible feature.
Here's list of some of those features:

* Hot-reloading\
   Currently there are no plans to support hot-reloading.

## Features

All out-of-the-box functionality exept core traits and types lives in their own `goods-*` crates.

### Sources

* [`goods-dataurl`] - providesx [`DataUrlSource`] - reads data embeded directly to url.
* [`goods-fs`] - provides adds [`FileSource`] - loads asset bytes from file-system.
* [`goods-reqwest`] - provides [`ReqwestSource`] - loads asset bytes from URLs using [`reqwest`].
  [`tokio`] runtime should be used to poll futures in this case.
* [`goods-fetch`] - provides [`FetchSource`] - uses browser's Fetch API to load assets data.

### Formats

* [`goods-json`] - provides [`JsonFormat`] - treats raw bytes as JSON document and deserializes asset representation via serde
* [`goods-yaml`] - provides [`YamlFormat`] - treats raw bytes as YAML document and deserializes asset representation via serde
* [`goods-ron`] - provides [`RonFormat`] - treats raw bytes as RON document and deserializes asset representation via serde

## Examples

There are few simple examples provided to learn how use this crate.

### [fs examlple](./examples/src/dataurl.rs)
Shows how to build registry with [`DataUrlSource`] and load simple assets from it.

### [fs examlple](./examples/src/fs.rs)
Shows how to build registry with [`FileSource`] and load simple assets from it.

### [reqwest example](./examples/src/reqwest.rs)
Async example that loads assets using HTTP protocol with [`tokio`] and [`reqwest`] crates.

### [fetch example](./examples/src/fetch.rs)
Shows how to load assets in browser using Fetch API.

This example can be built using [build-wasm32.sh](./examples/build-wasm32.sh) or [build-wasm32.bat](./examples/build-wasm32.bat) in [examples](./examples) directory.\
[`wasm-bindgen`] (compatible version) must be in `PATH`

```sh
cd examples
build-wasm32 fetch
python3 server.py
```

Then open http://localhost:8000/fetch.html in your favorite browser.
Loaded assets must be shown on the page. Otherwise see for errors in log.

## License

This repository is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution Licensing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[`goods`]: https://docs.rs/goods
[`goods-json`]: https://docs.rs/goods-json
[`goods-yaml`]: https://docs.rs/goods-yaml
[`goods-ron`]: https://docs.rs/goods-ron
[`goods-dataurl`]: https://docs.rs/goods-dataurl
[`goods-fs`]: https://docs.rs/goods-fs
[`goods-fetch`]: https://docs.rs/goods-fetch
[`goods-reqwest`]: https://docs.rs/goods-reqwest

[`Format`]: https://docs.rs/goods/latest/goods/trait.Format.html
[`Source`]: https://docs.rs/goods/latest/goods/trait.Source.html
[`Source::read`]: https://docs.rs/goods/latest/goods/trait.Source.html#tymethod.read

[`JsonFormat`]: https://docs.rs/goods-json/latest/goods-json/struct.JsonFormat.html
[`YamlFormat`]: https://docs.rs/goods-yaml/latest/goods-yaml/struct.YamlFormat.html
[`RonFormat`]: https://docs.rs/goods-ron/latest/goods-ron/struct.RonFormat.html

[`DataUrlSource`]: https://docs.rs/goods-fs/latest/goods-dataurl/struct.DataUrlSource.html
[`FileSource`]: https://docs.rs/goods-fs/latest/goods-fs/struct.FileSource.html
[`FetchSource`]: https://docs.rs/goods-fetch/latest/goods-fetch/struct.FetchSource.html
[`ReqwestSource`]: https://docs.rs/goods-reqwest/latest/goods-reqwest/struct.ReqwestSource.html

[`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
[`Sync`]: https://doc.rust-lang.org/std/marker/trait.Sync.html
[`serde::de::DeserializeOwned`]: https://docs.rs/serde/1/serde/de/trait.DeserializeOwned.html
[`tokio`]: https://tokio.rs/
[`reqwest`]: https://docs.rs/reqwest
[`wasm-bindgen`]: https://github.com/rustwasm/wasm-bindgen
