#![cfg(target_os = "wasi")]

pub use {
    goods_treasury_import::{
        eyre,
        ffi::{
            treasury_importer_alloc, treasury_importer_dealloc,
            treasury_importer_import_trampoline, treasury_importer_name_source_native_trampoline,
        },
        generate_imports_and_exports, Importer, Registry,
    },
    std::path::Path,
};

pub struct PluginImporter;

impl Importer for PluginImporter {
    fn name(&self) -> &str {
        "plugin"
    }

    fn source(&self) -> &str {
        "source"
    }

    fn native(&self) -> &str {
        "native"
    }

    fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        _registry: &mut dyn Registry,
    ) -> eyre::Result<()> {
        std::fs::copy(source_path, native_path)?;

        Ok(())
    }
}

generate_imports_and_exports! {
    &PluginImporter
}
