use {
    goods_import::{Importer, Registry},
    std::{error::Error, path::Path},
};

/// This is required to minimize chances that random shared library
/// would export symbols with same name and cause UB.
/// If magic number does not match shared library won't be used.
#[allow(non_upper_case_globals)]
#[no_mangle]
pub static goods_import_magic_number: u32 = goods_import::MAGIC;

/// Import version to check that both rustc version and `goods-import` dependency version
/// match. Otherwise using `get_goods_importers` may cause UB.
#[no_mangle]
pub fn get_goods_import_version() -> &'static str {
    goods_import::goods_import_version()
}

/// Returns array of importers from this library.
#[no_mangle]
pub fn get_goods_importers() -> Vec<Box<dyn Importer>> {
    vec![Box::new(DummyImporter)]
}

/// Dummy importer that imports assets as-is.
/// Contrary to intuition this is almost always NOT what app needs.
/// In most circumstances conversion or at least validation is required.
struct DummyImporter;

impl Importer for DummyImporter {
    /// Name of the importer.
    /// Prefer to include library identifier into name to avoid collisions.
    fn name(&self) -> &str {
        "dummy"
    }

    /// Import asset from source path.
    /// Save to native path.
    /// Register sub-assets if necessary.
    fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        _registry: &mut dyn Registry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // This simply creates hard-link on most platforms.
        match std::fs::copy(source_path, native_path) {
            Ok(_) => Ok(()),
            Err(err) => Err(Box::new(err)),
        }
    }
}
