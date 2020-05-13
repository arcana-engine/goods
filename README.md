# Goods - asset manager
Loads only good assets.

## Goals

This crate is written with following goals in mind:

* **Batteries included.**\
  Crate comes with variety of simple data sources like `FileSource` and `HttpSource`.
  Few `Format`s based on serde are included under feature flags.
* **Extensibility.**\
  Multiple `Format` traits can be implemented for any asset type, including foreign asset types.\
  For example `JsonFormat`, `YamlFormat` and `RonFormat` (bundled in the crate) implement `Format` trait for any asset type
  which intermediate representation implements `serde::de::DeserializeOwned`.
* **Supporting WebAssembly.**\
  All mandatory dependencies are WASM-compatible and no threading is required for asset loading to work.
* **Working with asynchronous data sources.**\
  All data sources implement `Source` trait.
  `Source::read` method returns future that will be driven to completion by the bound `Loader`.
* **no_std**\
    `alloc` is required.
* **Fast compilation.**\
    build after `cargo clean` takes ~3s.

## Non-Goals

This crate is not aimed to support every possible feature.
Here's list of some of those features:

* Hot-reloading
  Currently there are no plans to support hot-reloading.

## Features

All out-of-the-box functionality execut core traits and types are enabled with features.

* `std` - adds implementation of `std::error::Error` trait for error types.
* `fs` (enables `std`) - adds `FileSource` - `Source` implementation that loads asset bytes from file-system.
* `reqwest` - adds `ReqwestSource` - `Source` implementation that loads asset bytes from URLs using `reqwest`.
  Using this source requires `Loader` to be polled by `tokio`. Otherwise `reqwest` interals will panic.
* `json-format` - adds `Format` implementation that treats asset bytes as JSON document and deserializes asset representation via serde
* `yaml-format` - adds `Format` implementation that treats asset bytes as YAML document and deserializes asset representation via serde
* `ron-format` - adds `Format` implementation that treats asset bytes as RON document and deserializes asset representation via serde

## Examples

There are few simple examples provided already.

[fs examlple](./examples/fs.rs) - shows how to build registry with `FileSource` and load simple assets from it.

[reqwest example](./examples/reqwest.rs) - async example that loads assets from `localhost` over HTTP using `tokio` and `reqwest` crates.

[legion example](./examples/legion.rs) - shows how to load assets into entity using `legion` ECS crate.

## License

This repository is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution Licensing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
