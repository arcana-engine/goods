//!
//! Reliquary helps keeping asset importing code away from app and
//! address assets with uuids instead of error-prone file paths and URLs.
//!
//! Importers can be loaded from dylib crates. See [`dummy`] crate for example
//!
//! TODO: Ability to archive selected assets
//!
//!
//! [`relictl`] - CLI tool can be used to create reliquary instances, register assets and checks loading-importing process.
//!
//! [dummy]: https://github.com/zakarumych/reliquary/tree/master/dummy
//! [`relictl`]: https://github.com/zakarumych/reliquary/tree/master/relictl

use {
    reliquary_import::{reliquary_import_version, Importer},
    std::{
        collections::{
            hash_map::{Entry, HashMap},
            HashSet,
        },
        error::Error,
        io::Read,
        path::{Path, PathBuf},
        sync::Arc,
    },
    uuid::Uuid,
};

/// Contains meta-information about an asset.
#[derive(serde::Serialize, serde::Deserialize)]
struct Relic {
    /// Id of the relic.
    uuid: Uuid,

    /// Relic version
    /// Incremented with each reimport.
    version: u64,

    /// Path to source file.
    source: Arc<Path>,

    /// Importer for the relic.
    importer: Arc<str>,

    /// Path to native file.
    native: Option<Arc<Path>>,

    /// Arrays of tags associated with the relic.
    tags: Box<[Arc<str>]>,
}

/// Storage for relics.
pub struct Reliquary {
    registry: Registry,
    importers: HashMap<Box<str>, ImporterEntry>,
}

struct ImporterEntry {
    importer: Box<dyn Importer>,
    _lib: Arc<dlopen::raw::Library>,
}

/// Storage for relicts.
#[derive(serde::Serialize, serde::Deserialize)]
struct Registry {
    sources_root: Arc<Path>,
    natives_root: Arc<Path>,
    relics: Vec<Relic>,
    importer_dirs: HashSet<PathBuf>,

    #[serde(skip)]
    tags: HashSet<Arc<str>>,

    #[serde(skip)]
    relics_by_uuid: HashMap<Uuid, usize>,

    #[serde(skip)]
    relics_by_source: HashMap<Arc<Path>, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("Failed to save reliquary file")]
    IoError(#[source] std::io::Error),
    #[error("Failed to serialize reliquary file")]
    BincodeError(#[source] bincode::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("Failed to open reliquary file")]
    ReliquaryFile(#[source] std::io::Error),

    #[error("Failed to deserialize reliquary file")]
    BincodeError(#[source] bincode::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("Relic not found")]
    NotFound,

    #[error("Importer not found for the asset")]
    ImporterNotFound,

    #[error("Import failed")]
    ImporterError(#[source] Box<dyn Error + Send + Sync>),

    #[error("Failed to access source file")]
    SourceIoError(#[source] std::io::Error),

    #[error("Failed to access native file")]
    NativeIoError(#[source] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("Failed to access source file")]
    SourceIoError(#[source] std::io::Error),
}

impl Reliquary {
    #[tracing::instrument(skip(sources, natives), fields(sources = %sources.as_ref().display(), natives = %natives.as_ref().display()))]
    pub fn new(sources: impl AsRef<Path>, natives: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Reliquary {
            registry: Registry {
                sources_root: sources.as_ref().canonicalize()?.into(),
                natives_root: natives.as_ref().canonicalize()?.into(),
                relics: Vec::new(),
                importer_dirs: HashSet::new(),
                relics_by_source: HashMap::new(),
                relics_by_uuid: HashMap::new(),
                tags: HashSet::new(),
            },
            importers: HashMap::new(),
        })
    }

    #[tracing::instrument(skip(self, dir_path), fields(dir_path = %dir_path.as_ref().display()))]
    pub fn load_importers(&mut self, dir_path: impl AsRef<Path>) -> std::io::Result<()> {
        let dir_path = dir_path.as_ref();
        if !self.registry.importer_dirs.contains(dir_path) {
            load_importers_dir(&mut self.importers, dir_path)?;
            self.registry.importer_dirs.insert(dir_path.into());
        }
        Ok(())
    }

    /// Opens reliquary from file.
    #[tracing::instrument(skip(path), fields(path = %path.as_ref().display()))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenError> {
        let file = std::fs::File::open(path).map_err(OpenError::ReliquaryFile)?;
        let mut registry: Registry =
            bincode::deserialize_from(file).map_err(OpenError::BincodeError)?;

        for relic in &mut registry.relics {
            for tag in &mut *relic.tags {
                match registry.tags.get(&**tag) {
                    Some(t) => *tag = t.clone(),
                    None => {
                        registry.tags.insert(tag.clone());
                    }
                }
            }
        }

        registry.relics_by_uuid = registry
            .relics
            .iter()
            .enumerate()
            .map(|(index, info)| (info.uuid, index))
            .collect();

        registry.relics_by_source = registry
            .relics
            .iter()
            .enumerate()
            .map(|(index, info)| (info.source.clone(), index))
            .collect();

        let mut importers = HashMap::new();
        for dir_path in &registry.importer_dirs {
            if let Err(err) = load_importers_dir(&mut importers, dir_path) {
                tracing::error!("Failed to scan importers dir: {:#}", err);
            }
        }

        Ok(Reliquary {
            registry,
            importers,
        })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SaveError> {
        let file = std::fs::File::create(path).map_err(SaveError::IoError)?;
        bincode::serialize_into(file, &self.registry).map_err(SaveError::BincodeError)
    }

    /// Registers asset in this reliquary
    pub fn store(
        &mut self,
        source: impl AsRef<Path>,
        importer: &str,
        tags: &[&str],
    ) -> Result<Uuid, RegisterError> {
        self.registry.register(source, importer, tags)
    }

    /// Fetches asset in native format.
    /// Performs conversion if native format is absent or out of date.
    #[tracing::instrument(skip(self))]
    pub fn fetch(&mut self, uuid: Uuid) -> Result<Box<[u8]>, FetchError> {
        match self.registry.relics_by_uuid.get(&uuid) {
            None => Err(FetchError::NotFound),
            Some(&index) => {
                let relic = &self.registry.relics[index];

                let source_path = self.registry.sources_root.join(&relic.source);

                let source_file =
                    std::fs::File::open(&source_path).map_err(FetchError::SourceIoError)?;

                match &relic.native {
                    Some(native) => {
                        let native_path = self.registry.natives_root.join(native);

                        let source_modified = source_file
                            .metadata()
                            .and_then(|m| m.modified())
                            .map_err(FetchError::SourceIoError)?;

                        let mut native_file =
                            std::fs::File::open(&native_path).map_err(FetchError::NativeIoError)?;

                        let native_modifier = native_file
                            .metadata()
                            .and_then(|m| m.modified())
                            .map_err(FetchError::NativeIoError)?;

                        if native_modifier >= source_modified {
                            tracing::trace!("Native asset file is up-to-date");
                            let mut native_data = Vec::new();
                            native_file
                                .read_to_end(&mut native_data)
                                .map_err(FetchError::NativeIoError)?;

                            return Ok(native_data.into_boxed_slice());
                        }

                        tracing::trace!("Native asset file is out-of-date. Perform reimport");

                        match self.importers.get(&*relic.importer) {
                            None => {
                                let mut native_data = Vec::new();
                                native_file
                                    .read_to_end(&mut native_data)
                                    .map_err(FetchError::NativeIoError)?;
                                Ok(native_data.into_boxed_slice())
                            }

                            Some(importer) => {
                                let native_path_tmp = native_path.with_extension("tmp");

                                match importer.importer.import(
                                    &source_path,
                                    &native_path_tmp,
                                    &mut self.registry,
                                ) {
                                    Ok(()) => {
                                        tracing::trace!("Native file updated");

                                        std::fs::rename(&native_path_tmp, &native_path)
                                            .map_err(FetchError::NativeIoError)?;

                                        let relic = &mut self.registry.relics[index];
                                        relic.version += 1;
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            "Native file reimport failed '{:#}'. Fallback to old file", err
                                        );
                                    }
                                }
                                Ok(std::fs::read(&native_path)
                                    .map_err(FetchError::NativeIoError)?
                                    .into_boxed_slice())
                            }
                        }
                    }
                    None => {
                        tracing::trace!("Native asset file is absent. Import is required");
                        match self.importers.get(&*relic.importer) {
                            None => Err(FetchError::ImporterNotFound),

                            Some(importer) => {
                                let native = relic.uuid.to_hyphenated().to_string();
                                let native_path = self.registry.natives_root.join(&native);

                                let native_path_tmp = native_path.with_extension("tmp");

                                importer
                                    .importer
                                    .import(&source_path, &native_path_tmp, &mut self.registry)
                                    .map_err(FetchError::ImporterError)?;

                                tracing::trace!("Native file imported");

                                std::fs::rename(&native_path_tmp, &native_path)
                                    .map_err(FetchError::NativeIoError)?;

                                let data = std::fs::read(&native_path)
                                    .map_err(FetchError::NativeIoError)?
                                    .into_boxed_slice();

                                let relic = &mut self.registry.relics[index];
                                relic.native = Some(PathBuf::from(native).into());
                                Ok(data)
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Registry {
    fn register(
        &mut self,
        source: impl AsRef<Path>,
        importer: &str,
        tags: &[&str],
    ) -> Result<Uuid, RegisterError> {
        let source = source.as_ref();

        if let Some(&index) = self.relics_by_source.get(source) {
            return Ok(self.relics[index].uuid);
        }

        let uuid = loop {
            let uuid = Uuid::new_v4();
            if !self.relics_by_uuid.contains_key(&uuid) {
                break uuid;
            }
        };

        let tags = tags
            .iter()
            .map(|&tag| {
                self.tags.get(tag).cloned().unwrap_or_else(|| {
                    let tag = Arc::from(tag);
                    self.tags.insert(Arc::clone(&tag));
                    tag
                })
            })
            .collect();

        self.relics.push(Relic {
            uuid,
            version: 0,
            source: source.into(),
            native: None,
            importer: importer.into(),
            tags,
        });

        Ok(uuid)
    }
}

impl reliquary_import::Reliquary for Registry {
    fn store(
        &mut self,
        source: &Path,
        importer: &str,
    ) -> Result<Uuid, Box<dyn Error + Send + Sync>> {
        match self.register(source, importer, &[]) {
            Ok(uuid) => Ok(uuid),
            Err(err) => Err(Box::new(err)),
        }
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
                    err
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum LoadImportersError {
    #[error("Failed to open importer library")]
    Dlopen(#[from] dlopen::Error),

    #[error("Not a reliquary importer library")]
    InvalidLibrary,

    #[error(
        "Magic number from importer library '{magic:0x}' does not match expected value '0xe11c9a87'"
    )]
    WrongMagic { magic: u32 },

    #[error("Version of `reliquary-import` does not match")]
    WrongVersion,
}

fn load_importers(
    importers: &mut HashMap<Box<str>, ImporterEntry>,
    lib_path: &Path,
) -> Result<(), LoadImportersError> {
    let lib = dlopen::raw::Library::open(lib_path)?;

    let lib = Arc::new(lib);

    let result = unsafe {
        // This is not entirely safe. Random shared library can export similar symbol
        // which has different type.
        // Checking magic number is nice to avoid problems from random shared library.
        // While malicious library can export functions and trigger UB there,
        // linking to one is akin to depending on unsound library.
        // Hint: Don't have malicious shared libraries in your system.
        lib.symbol::<&u32>("reliquary_import_magic_number")
    };

    match result {
        Ok(magic) if *magic == reliquary_import::MAGIC => {}
        Ok(magic) => {
            tracing::error!("Wrong `reliquary_import_magic_number`");
            return Err(LoadImportersError::WrongMagic { magic: *magic });
        }
        Err(err) => {
            tracing::error!("`reliquary_import_magic_number` symbol not found");
            return Err(LoadImportersError::Dlopen(err));
        }
    }

    let result = unsafe {
        // This is not entirely safe. Random shared library can export similar symbol
        // which has different type.
        lib.symbol::<fn() -> &'static str>("get_reliquary_import_version")
    };

    match result {
        Ok(get_reliquary_import_version) => {
            let expected_reliquary_import_version = reliquary_import_version();
            let library_reliquary_import_version = get_reliquary_import_version();

            if library_reliquary_import_version == expected_reliquary_import_version {
                match unsafe {
                    lib.symbol::<fn() -> Vec<Box<dyn Importer>>>("get_reliquary_importers")
                } {
                    Ok(get_reliquary_importers) => {
                        for importer in get_reliquary_importers() {
                            match importers.entry(importer.name().into()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(ImporterEntry {
                                        importer,
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
                        Err(LoadImportersError::Dlopen(err))
                    }
                }
            } else {
                tracing::warn!(
                    "Importer plugin '{}' has outdated `reliquary-import` dependency version. Expected: {}, Library: {}",
                    lib_path.display(),
                    expected_reliquary_import_version,
                    library_reliquary_import_version,
                );
                Err(LoadImportersError::WrongVersion)
            }
        }
        Err(err) => {
            tracing::error!(
                "Failed to get `get_reliquary_import_version` function from importer library"
            );
            Err(LoadImportersError::Dlopen(err))
        }
    }
}
