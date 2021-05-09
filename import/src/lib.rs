use {
    std::{error::Error, path::Path},
    uuid::Uuid,
};

/// Object to register sub-assets when importing super-asset.
pub trait Registry {
    /// Register sub-asset at source path, assigning specified importer.
    fn store(
        &mut self,
        source: &Path,
        importer: &str,
    ) -> Result<Uuid, Box<dyn Error + Send + Sync>>;
}

pub trait Importer: Send + Sync {
    /// Returns name of the importer
    fn name(&self) -> &str;

    /// Imports asset from source file, saving result to native file.
    /// Register sub-assets if necessary.
    fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        registry: &mut dyn Registry,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}

/// Magic number to export as `goods_import_magic_number` in importer library.
pub const MAGIC: u32 = 0xe11c9a87;

/// Returns combination of rustc version and this crate version.
/// Must be used in `get_goods_import_version` function exported by importer libraries.
pub fn goods_import_version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        "@",
        env!("GOODS_IMPORT_RUSTC_VERSION")
    )
}
