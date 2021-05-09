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

use {
    goods_import::{goods_import_version, Importer},
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
struct Asset {
    /// Id of the asset.
    uuid: Uuid,

    /// Asset version
    /// Incremented with each reimport.
    version: u64,

    /// Path to source file.
    source: Arc<Path>,

    /// Importer for the asset.
    importer: Arc<str>,

    /// Path to native file.
    native: Option<Arc<Path>>,

    /// Arrays of tags associated with the asset.
    tags: Box<[Arc<str>]>,
}

/// Storage for goods.
pub struct Goods {
    registry: Registry,
    importers: HashMap<Box<str>, ImporterEntry>,
}

struct ImporterEntry {
    importer: Box<dyn Importer>,
    _lib: Arc<dlopen::raw::Library>,
}

/// Storage for goods.
#[derive(serde::Serialize, serde::Deserialize)]
struct Registry {
    sources_root: Arc<Path>,
    natives_root: Arc<Path>,
    assets: Vec<Asset>,
    importer_dirs: HashSet<PathBuf>,

    #[serde(skip)]
    tags: HashSet<Arc<str>>,

    #[serde(skip)]
    assets_by_uuid: HashMap<Uuid, usize>,

    #[serde(skip)]
    assets_by_source: HashMap<Arc<Path>, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("Failed to save goods file")]
    IoError(#[source] std::io::Error),
    #[error("Failed to serialize goods file")]
    BincodeError(#[source] bincode::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("Failed to open goods file")]
    GoodsFile(#[source] std::io::Error),

    #[error("Failed to deserialize goods file")]
    BincodeError(#[source] bincode::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("Asset not found")]
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

pub struct AssetData {
    pub bytes: Box<[u8]>,
    pub version: u64,
}

impl Goods {
    #[tracing::instrument(skip(sources, natives), fields(sources = %sources.as_ref().display(), natives = %natives.as_ref().display()))]
    pub fn new(sources: impl AsRef<Path>, natives: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Goods {
            registry: Registry {
                sources_root: sources.as_ref().canonicalize()?.into(),
                natives_root: natives.as_ref().canonicalize()?.into(),
                assets: Vec::new(),
                importer_dirs: HashSet::new(),
                assets_by_source: HashMap::new(),
                assets_by_uuid: HashMap::new(),
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

    /// Opens goods from file.
    #[tracing::instrument(skip(path), fields(path = %path.as_ref().display()))]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenError> {
        let file = std::fs::File::open(path).map_err(OpenError::GoodsFile)?;
        let mut registry: Registry =
            bincode::deserialize_from(file).map_err(OpenError::BincodeError)?;

        for asset in &mut registry.assets {
            for tag in &mut *asset.tags {
                match registry.tags.get(&**tag) {
                    Some(t) => *tag = t.clone(),
                    None => {
                        registry.tags.insert(tag.clone());
                    }
                }
            }
        }

        registry.assets_by_uuid = registry
            .assets
            .iter()
            .enumerate()
            .map(|(index, info)| (info.uuid, index))
            .collect();

        registry.assets_by_source = registry
            .assets
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

        Ok(Goods {
            registry,
            importers,
        })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SaveError> {
        let file = std::fs::File::create(path).map_err(SaveError::IoError)?;
        bincode::serialize_into(file, &self.registry).map_err(SaveError::BincodeError)
    }

    /// Registers asset in this goods
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
    pub fn fetch(&mut self, uuid: &Uuid) -> Result<AssetData, FetchError> {
        Ok(self.fetch_impl(uuid, 0)?.unwrap())
    }

    /// Fetches asset in native format.
    /// Returns `Ok(None)` if native file is up-to-date.
    /// Performs conversion if native format is absent or out of date.
    #[tracing::instrument(skip(self))]
    pub fn fetch_updated(
        &mut self,
        uuid: &Uuid,
        version: u64,
    ) -> Result<Option<AssetData>, FetchError> {
        self.fetch_impl(uuid, version + 1)
    }

    pub fn preimport(&mut self) {
        macro_rules! try_continue {
            ($e:expr) => {
                match $e {
                    Ok(val) => val,
                    Err(_) => continue,
                }
            };
        }

        for index in 0.. {
            if index >= self.registry.assets.len() {
                break;
            }

            let asset = &self.registry.assets[index];
            let source_path = self.registry.sources_root.join(&asset.source);

            match &asset.native {
                Some(native) => {
                    let native_path = self.registry.natives_root.join(native);

                    let source_modified =
                        try_continue!(std::fs::metadata(&source_path).and_then(|m| m.modified()));

                    let native_modified =
                        try_continue!(std::fs::metadata(&native_path).and_then(|m| m.modified()));

                    if native_modified >= source_modified {
                        tracing::trace!("Native asset file is up-to-date");
                        continue;
                    }

                    tracing::trace!("Native asset file is out-of-date. Perform reimport");

                    match self.importers.get(&*asset.importer) {
                        None => {
                            tracing::warn!(
                                "Importer '{}' not found, asset '{}@{}' cannot be updated",
                                asset.importer,
                                asset.uuid,
                                asset.source.display(),
                            );
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
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        "Native file reimport failed '{:#}'. Fallback to old file",
                                        err
                                    );
                                }
                            }
                        }
                    }
                }
                None => {
                    tracing::trace!("Native asset file is absent. Import is required");
                    match self.importers.get(&*asset.importer) {
                        None => {
                            tracing::warn!(
                                "Importer '{}' not found, asset '{}@{}' cannot be initialized",
                                asset.importer,
                                asset.uuid,
                                asset.source.display(),
                            );
                        }

                        Some(importer) => {
                            let native = asset.uuid.to_hyphenated().to_string();
                            let native_path = self.registry.natives_root.join(&native);

                            let native_path_tmp = native_path.with_extension("tmp");

                            try_continue!(importer.importer.import(
                                &source_path,
                                &native_path_tmp,
                                &mut self.registry
                            ));

                            try_continue!(std::fs::rename(&native_path_tmp, &native_path));

                            let asset = &mut self.registry.assets[index];
                            asset.version += 1;
                            asset.native = Some(PathBuf::from(native).into());

                            tracing::trace!("Native file imported");
                        }
                    }
                }
            }
        }
    }

    fn fetch_impl(
        &mut self,
        uuid: &Uuid,
        next_version: u64,
    ) -> Result<Option<AssetData>, FetchError> {
        match self.registry.assets_by_uuid.get(uuid) {
            None => Err(FetchError::NotFound),
            Some(&index) => {
                let asset = &self.registry.assets[index];

                let source_path = self.registry.sources_root.join(&asset.source);

                match &asset.native {
                    Some(native) => {
                        let native_path = self.registry.natives_root.join(native);

                        let source_modified = std::fs::metadata(&source_path)
                            .and_then(|m| m.modified())
                            .map_err(FetchError::SourceIoError)?;

                        let mut native_file =
                            std::fs::File::open(&native_path).map_err(FetchError::NativeIoError)?;

                        let native_modified = native_file
                            .metadata()
                            .and_then(|m| m.modified())
                            .map_err(FetchError::NativeIoError)?;

                        if native_modified >= source_modified {
                            tracing::trace!("Native asset file is up-to-date");
                            if next_version > asset.version {
                                return Ok(None);
                            }

                            let mut native_data = Vec::new();
                            native_file
                                .read_to_end(&mut native_data)
                                .map_err(FetchError::NativeIoError)?;

                            return Ok(Some(AssetData {
                                bytes: native_data.into_boxed_slice(),
                                version: asset.version,
                            }));
                        }

                        tracing::trace!("Native asset file is out-of-date. Perform reimport");

                        match self.importers.get(&*asset.importer) {
                            None => {
                                tracing::warn!(
                                    "Importer '{}' not found, asset '{}@{}' cannot be updated",
                                    asset.importer,
                                    asset.uuid,
                                    asset.source.display(),
                                );
                                if next_version > asset.version {
                                    return Ok(None);
                                }
                                let mut native_data = Vec::new();
                                native_file
                                    .read_to_end(&mut native_data)
                                    .map_err(FetchError::NativeIoError)?;

                                Ok(Some(AssetData {
                                    bytes: native_data.into_boxed_slice(),
                                    version: asset.version,
                                }))
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

                                        let asset = &mut self.registry.assets[index];
                                        asset.version += 1;

                                        if asset.version < next_version {
                                            tracing::warn!(
                                                "Attempt to fetch updated asset with last known version greater than actual asset version"
                                            )
                                        }
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            "Native file reimport failed '{:#}'. Fallback to old file", err
                                        );
                                    }
                                }

                                let asset = &self.registry.assets[index];
                                if next_version > asset.version {
                                    Ok(None)
                                } else {
                                    Ok(Some(AssetData {
                                        bytes: std::fs::read(&native_path)
                                            .map_err(FetchError::NativeIoError)?
                                            .into_boxed_slice(),
                                        version: asset.version,
                                    }))
                                }
                            }
                        }
                    }
                    None => {
                        tracing::trace!("Native asset file is absent. Import is required");
                        match self.importers.get(&*asset.importer) {
                            None => {
                                tracing::warn!(
                                    "Importer '{}' not found, asset '{}@{}' cannot be initialized",
                                    asset.importer,
                                    asset.uuid,
                                    asset.source.display(),
                                );
                                Err(FetchError::ImporterNotFound)
                            }

                            Some(importer) => {
                                let native = asset.uuid.to_hyphenated().to_string();
                                let native_path = self.registry.natives_root.join(&native);

                                let native_path_tmp = native_path.with_extension("tmp");

                                importer
                                    .importer
                                    .import(&source_path, &native_path_tmp, &mut self.registry)
                                    .map_err(FetchError::ImporterError)?;

                                std::fs::rename(&native_path_tmp, &native_path)
                                    .map_err(FetchError::NativeIoError)?;

                                let asset = &mut self.registry.assets[index];
                                asset.version += 1;
                                asset.native = Some(PathBuf::from(native).into());

                                tracing::trace!("Native file imported");

                                if next_version > asset.version {
                                    Ok(None)
                                } else {
                                    Ok(Some(AssetData {
                                        bytes: std::fs::read(&native_path)
                                            .map_err(FetchError::NativeIoError)?
                                            .into_boxed_slice(),
                                        version: asset.version,
                                    }))
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn fetch_frozen(&self, uuid: &Uuid) -> Result<Option<AssetData>, FetchError> {
        match self.registry.assets_by_uuid.get(uuid) {
            None => Err(FetchError::NotFound),
            Some(&index) => {
                let asset = &self.registry.assets[index];

                match &asset.native {
                    Some(native) => {
                        let native_path = self.registry.natives_root.join(native);

                        let mut native_file =
                            std::fs::File::open(&native_path).map_err(FetchError::NativeIoError)?;

                        let mut native_data = Vec::new();
                        native_file
                            .read_to_end(&mut native_data)
                            .map_err(FetchError::NativeIoError)?;

                        return Ok(Some(AssetData {
                            bytes: native_data.into_boxed_slice(),
                            version: asset.version,
                        }));
                    }
                    None => Ok(None),
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

        if let Some(&index) = self.assets_by_source.get(source) {
            return Ok(self.assets[index].uuid);
        }

        let uuid = loop {
            let uuid = Uuid::new_v4();
            if !self.assets_by_uuid.contains_key(&uuid) {
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

        self.assets.push(Asset {
            uuid,
            version: 0,
            source: source.into(),
            native: None,
            importer: importer.into(),
            tags,
        });

        self.assets_by_uuid.insert(uuid, self.assets.len() - 1);
        self.assets_by_source
            .insert(source.into(), self.assets.len() - 1);

        Ok(uuid)
    }
}

impl goods_import::Registry for Registry {
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
                    eyre::Report::from(err),
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

    #[error("Not a goods importer library")]
    InvalidLibrary,

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
    let lib = dlopen::raw::Library::open(lib_path)?;

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
        Ok(magic) if *magic == goods_import::MAGIC => {}
        Ok(magic) => {
            tracing::error!("Wrong `goods_import_magic_number`");
            return Err(LoadImportersError::WrongMagic { magic: *magic });
        }
        Err(err) => {
            tracing::error!("`goods_import_magic_number` symbol not found");
            return Err(LoadImportersError::Dlopen(err));
        }
    }

    let result = unsafe {
        // This is not entirely safe. Random shared library can export similar symbol
        // which has different type.
        lib.symbol::<fn() -> &'static str>("get_goods_import_version")
    };

    match result {
        Ok(get_goods_import_version) => {
            let expected = goods_import_version();
            let library = get_goods_import_version();

            if expected == library {
                match unsafe { lib.symbol::<fn() -> Vec<Box<dyn Importer>>>("get_goods_importers") }
                {
                    Ok(get_goods_importers) => {
                        for importer in get_goods_importers() {
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
                    "Importer plugin '{}' has outdated `goods-import` dependency version. Expected: {}, Library: {}",
                    lib_path.display(),
                    expected,
                    library,
                );
                Err(LoadImportersError::WrongVersion)
            }
        }
        Err(err) => {
            tracing::error!("Failed to get `goods_import_version` function from importer library");
            Err(LoadImportersError::Dlopen(err))
        }
    }
}
