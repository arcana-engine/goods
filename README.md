# Goods - Asset Pipeline

[![crates](https://img.shields.io/crates/v/goods.svg?style=for-the-badge&label=goods)](https://crates.io/crates/goods)
[![docs](https://img.shields.io/badge/docs.rs-goods-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/goods)
[![actions](https://img.shields.io/github/workflow/status/arcana-engine/goods/badge/master?style=for-the-badge)](https://github.com/arcana-engine/goods/actions?query=workflow%3ARust)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/arcana-engine/goods?style=for-the-badge)


`Goods` is an asset pipeline that helps keeping asset importing code away from the app\
and allows addressing assets with uuids instead of error-prone file paths and urls.

`Goods` provides fully async loader. Loader can be augmented with user-defined source implementations,\
making it possible to load asset from any kind of storage.\
And thanks to async nature it can be both local and remote storages.

`Treasury` is an asset database.\
Once asset is imported it is given an `uuid` that can be used with provided out-of-the-box `TreasurySource` to load the assets.\
On import `Treasury` calls user-defined importer to convert asset from authoring format into engine-native format.\
Importers should be compiled into WASM library and placed into directory configured for importers lookup.\
Provided `plugin` crate is an example of how to write a plugin and export importers from it.

A CLI tool is provided to perform importing manually.\
Running `cargo install goods-treasury-cli` should install the tool. CLI executable name is `treasury`.

Engines using Goods pipeline are encouraged to support importing in their toolset.

## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
