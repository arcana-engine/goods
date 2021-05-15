use {
    std::{
        collections::hash_map::{Entry, HashMap},
        error::Error,
        path::{Path, PathBuf},
        sync::Arc,
    },
    treasury_import::{treasury_import_version, Importer, Registry},
};
pub struct Importers {
    /// Importers
    map: HashMap<Box<str>, ImporterEntry>,
}

#[derive(Clone)]
pub struct ImporterEntry {
    inner: Arc<dyn Importer>,
    _lib: Arc<dlopen::raw::Library>,
}

impl Importer for ImporterEntry {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn import(
        &self,
        source_path: &Path,
        native_path: &Path,
        registry: &mut dyn Registry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.import(source_path, native_path, registry)
    }
}

impl Importers {
    pub fn new() -> Self {
        Importers {
            map: HashMap::new(),
        }
    }

    pub fn from_dirs(dirs: impl IntoIterator<Item = impl AsRef<Path>>) -> Self {
        let mut map = HashMap::new();
        for dir in dirs {
            if let Err(err) = load_importers_dir(&mut map, dir.as_ref()) {
                tracing::error!("Failed to scan importers dir: {:#}", err);
            }
        }

        Importers { map }
    }

    pub fn load_dir(&mut self, dir: impl AsRef<Path>) -> std::io::Result<()> {
        load_importers_dir(&mut self.map, dir.as_ref())
    }

    pub fn get_importer(&self, name: &str) -> Option<impl Importer> {
        self.map.get(name).cloned()
    }
}

fn load_importers_dir(
    importers: &mut HashMap<Box<str>, ImporterEntry>,
    dir_path: &Path,
) -> std::io::Result<()> {
    let dir = std::fs::read_dir(dir_path)?;

    for e in dir {
        let e = e?;
        let path = PathBuf::from(e.file_name());
        let is_shared_library = path
            .extension()
            .map_or(false, |e| e == dlopen::utils::PLATFORM_FILE_EXTENSION);

        if is_shared_library {
            let lib_path = dir_path.join(path);
            if let Err(err) = load_importers(importers, &lib_path) {
                tracing::warn!(
                    "Failed to load importers from library {}: '{:#}'",
                    lib_path.display(),
                    eyre::Report::from(err),
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum LoadImportersError {
    #[error("Failed to load shared library")]
    OpenLibrary { source: dlopen::Error },

    #[error("Failed to find symbol in library")]
    FindSymbol { source: dlopen::Error },

    #[error(
        "Magic number from importer library '{magic:0x}' does not match expected value '0xe11c9a87'"
    )]
    WrongMagic { magic: u32 },

    #[error("Version of `goods-import` does not match")]
    WrongVersion,
}

fn load_importers(
    importers: &mut HashMap<Box<str>, ImporterEntry>,
    lib_path: &Path,
) -> Result<(), LoadImportersError> {
    let lib = dlopen::raw::Library::open(lib_path)
        .map_err(|source| LoadImportersError::OpenLibrary { source })?;

    let lib = Arc::new(lib);

    let result = unsafe {
        // This is not entirely safe. Random shared library can export similar symbol
        // which has different type.
        // Checking magic number is nice to avoid problems from random shared library.
        // While malicious library can export functions and trigger UB there,
        // linking to one is akin to depending on unsound library.
        // Hint: Don't have malicious shared libraries in your system.
        lib.symbol::<&u32>("goods_import_magic_number")
    };

    match result {
        Ok(magic) if *magic == treasury_import::MAGIC => {}
        Ok(magic) => {
            tracing::error!("Wrong `goods_import_magic_number`");
            return Err(LoadImportersError::WrongMagic { magic: *magic });
        }
        Err(err) => {
            tracing::error!("`goods_import_magic_number` symbol not found");
            return Err(LoadImportersError::FindSymbol { source: err });
        }
    }

    let result = unsafe {
        // This is not entirely safe. Random shared library can export similar symbol
        // which has different type.
        lib.symbol::<fn() -> &'static str>("get_treasury_import_version")
    };

    match result {
        Ok(get_treasury_import_version) => {
            let expected = treasury_import_version();
            let library = get_treasury_import_version();

            if expected == library {
                match unsafe { lib.symbol::<fn() -> Vec<Arc<dyn Importer>>>("get_goods_importers") }
                {
                    Ok(get_goods_importers) => {
                        for importer in get_goods_importers() {
                            match importers.entry(importer.name().into()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(ImporterEntry {
                                        inner: importer,
                                        _lib: lib.clone(),
                                    });
                                }
                                Entry::Occupied(_) => {
                                    tracing::warn!(
                                        "Duplicate importer name '{}'. Importer skipped",
                                        importer.name()
                                    );
                                }
                            }
                        }
                        tracing::info!("Added importers from {}", lib_path.display());
                        Ok(())
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to get `get_importers` function from importer library"
                        );
                        Err(LoadImportersError::FindSymbol { source: err })
                    }
                }
            } else {
                tracing::warn!(
                    "Importer plugin '{}' has outdated `goods-import` dependency version. Expected: {}, Library: {}",
                    lib_path.display(),
                    expected,
                    library,
                );
                Err(LoadImportersError::WrongVersion)
            }
        }
        Err(err) => {
            tracing::error!(
                "Failed to get `treasury_import_version` function from importer library"
            );
            Err(LoadImportersError::FindSymbol { source: err })
        }
    }
}
