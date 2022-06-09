# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0](https://www.github.com/arcana-engine/goods/compare/v0.6.0...v0.7.0) (2022-06-09)


### Features

* Add typed asset id ([bae61a3](https://www.github.com/arcana-engine/goods/commit/bae61a3942a3c81f0e6aa6951c8cf8ad361fbfa4))
* Serializable AssetId ([980344f](https://www.github.com/arcana-engine/goods/commit/980344f79b486bc81505429c6ee87c436e6df2b5))


### Bug Fixes

* clippy ([a0f0e7a](https://www.github.com/arcana-engine/goods/commit/a0f0e7afd483d50286264edb024a67cabfcd86bf))

## [Unreleased]

Changed:
* Assets are addressed by 64-bit IDs.
* `goods-treasury` is reimplemented from scratch as standalone family of crates in https://github.com/arcana-engine/treasury
  `treasury-client` crate is used to implement `Source` to fetch assets from treasuries.
* Assets can be addressed by string key.

## [0.9.0] - 2021-07-19

Total rework.
This is whole asset pipeline now.

`goods` crates remains a loader.
Added:
* Basic support for asset hot-reloading.
* Proc-macro for automatic definition of complex asset types.

Changed:
* Assets are addressed by UUIDs.
* Improved multi-threading support.

`goods-treasury` asset database with programmable importing and hot-reimporting.
