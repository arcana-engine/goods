//!
//! Goods helps keeping asset importing code away from app and
//! address assets with uuids instead of error-prone file paths and URLs.
//!
//! Importers can be loaded from dylib crates. See [`dummy`] crate for example
//!
//! TODO: Ability to archive selected assets
//!
//!
//! [`goods-cli`] - CLI tool can be used to create goods instances, register assets and checks loading-importing process.
//!
//! [dummy]: https://github.com/zakarumych/goods/tree/overhaul/dummy
//! [`goods-cli`]: https://github.com/zakarumych/goods/tree/overhaul/cli
//!

mod asset;
mod import;
mod treasury;

pub use {self::treasury::*, treasury_import::*};
