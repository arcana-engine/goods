# Goods - Asset Pipeline

[![crates](https://img.shields.io/crates/v/goods.svg?style=for-the-badge&label=goods)](https://crates.io/crates/goods)
[![docs](https://img.shields.io/badge/docs.rs-goods-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/goods)
[![actions](https://img.shields.io/github/workflow/status/arcana-engine/goods/badge/master?style=for-the-badge)](https://github.com/arcana-engine/goods/actions?query=workflow%3ARust)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/arcana-engine/goods?style=for-the-badge)


Goods is an asset system primarily designed for game engines.
It supports definition of complex assets using powerful derive-macros and asynchronous loading with trait-based asset sources.


## Definition

To define an asset users must implement `Asset` trait.
Type that implements `Asset` traits are called assets and their values are produced by asset loading process.

`Asset` trait is rather complex. Many of its parts looks like boilerplate when defining simple asset type.



## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
