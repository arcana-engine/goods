use {std::path::Path, uuid::Uuid};

#[cfg(feature = "ffi")]
pub mod ffi;

pub use eyre;

/// Object to register sub-assets when importing super-asset.
pub trait Registry {
    /// Register asset at source path, assigning specified importer.
    /// Source path must be absolute.
    fn store(
        &mut self,
        source: &Path,
        source_format: &str,
        native_format: &str,
        tags: &[&str],
    ) -> eyre::Result<Uuid>;

    /// Returns native path to asset with specified uuid.
    fn fetch(&mut self, asset: &Uuid) -> eyre::Result<Box<Path>>;
}

pub trait Importer: Send + Sync {
    /// Returns name of the importer
    fn name(&self) -> &str;

    /// Returns name of the source format
    fn source(&self) -> &str;

    /// Returns name of the native format
    fn native(&self) -> &str;

    /// Imports asset from source file, saving result to native file.
    /// Register sub-assets if necessary.
    fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        registry: &mut dyn Registry,
    ) -> eyre::Result<()>;
}
