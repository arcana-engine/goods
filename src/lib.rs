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
        time::SystemTime,
    },
    uuid::Uuid,
};

/// Contains meta-information about an asset.
#[derive(serde::Serialize, serde::Deserialize)]
struct Asset {
    /// Id of the asset.
    uuid: Uuid,

    /// Path to source file.
    /// Relative to root path.
    source: Arc<Path>,

    /// Importer for the asset.
    importer: Arc<str>,

    /// Arrays of tags associated with the asset.
    tags: Box<[Arc<str>]>,

    #[serde(skip, default = "default_absolute_path_arc")]
    native_absolute: Arc<Path>,

    /// Path to source file.
    #[serde(skip, default = "default_absolute_path_arc")]
    source_absolute: Arc<Path>,
}

/// Storage for goods.
pub struct Goods {
    inner: Inner,
}

struct Inner {
    /// All paths not suffixed with `_absolute` are relative to this.
    root: Box<Path>,

    // Data loaded from `root.join(".goods")`.
    data: GoodsData,

    /// Set of all tags.
    tags: HashSet<Arc<str>>,

    /// Lookup assets by uuid.
    assets_by_uuid: HashMap<Uuid, usize>,

    /// Lookup assets by source path and importer name combination.
    assets_by_source_importer: HashMap<(Arc<Path>, Arc<str>), usize>,

    /// Importers
    importers: HashMap<Box<str>, ImporterEntry>,
}

struct ImporterEntry {
    importer: Arc<dyn Importer>,
    _lib: Arc<dlopen::raw::Library>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GoodsData {
    /// Set of paths to directories from where importer libraries are loaded.
    importer_dirs: HashSet<Box<Path>>,

    /// Array with all registered assets.
    assets: Vec<Asset>,
}

#[derive(Debug, thiserror::Error)]
pub enum NewError {
    #[error("Goods path '{path}' is occupied")]
    GoodsAlreadyExist { path: Box<Path> },

    #[error("Goods path '{path}' is invalid")]
    InvalidGoodsPath { path: Box<Path> },

    #[error("Failed to canonicalize root directory '{path}'")]
    RootDirCanonError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Failed to create root directory '{path}'")]
    RootDirCreateError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Root '{path}' is not a directory")]
    RootIsNotDir { path: Box<Path> },

    #[error(transparent)]
    SaveError(#[from] SaveError),
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("Failed to open goods path '{path}'")]
    GoodsOpenError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Failed to deserialize goods file")]
    BincodeError {
        path: Box<Path>,
        source: bincode::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("Goods path '{path}' is invalid")]
    InvalidGoodsPath { path: Box<Path> },

    #[error("Failed to canonicalize root directory '{path}'")]
    RootDirCanonError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Failed to open goods path '{path}'")]
    GoodsOpenError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Failed to deserialize goods file")]
    BincodeError {
        path: Box<Path>,
        source: bincode::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Failed to run importer library")]
    ImporterLibraryRunError { source: std::io::Error },

    #[error("Importer library returned error")]
    ImporterLibraryError { stderr: String },

    #[error("Failed to parse importer response")]
    ImporterLibraryResponseParseError { source: serde_json::Error },
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("Asset not found")]
    NotFound,

    #[error("Failed to access native file '{path}'")]
    NativeIoError {
        path: Box<Path>,
        source: std::io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Failed to find importer '{importer}'")]
    ImporterNotFound { importer: String },

    #[error("Import failed")]
    ImportError {
        source: Box<dyn Error + Send + Sync>,
    },

    #[error("Failed to access source file '{path}'")]
    SourceIoError {
        path: Box<Path>,
        source: std::io::Error,
    },

    #[error("Failed to access native file '{path}'")]
    NativeIoError {
        path: Box<Path>,
        source: std::io::Error,
    },
}

pub struct AssetData {
    pub bytes: Box<[u8]>,
    pub version: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to scan importers directory '{path}'")]
pub struct ImporterDirError {
    path: Box<Path>,
    source: std::io::Error,
}

impl Goods {
    /// Create new goods storage.
    #[tracing::instrument(fields(root = %root.as_ref().display()))]
    pub fn new(root: impl AsRef<Path>, overwrite: bool) -> Result<Self, NewError> {
        let root = root.as_ref();

        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(|source| NewError::RootDirCreateError {
                source,
                path: root.into(),
            })?;
        } else if !root.is_dir() {
            return Err(NewError::RootIsNotDir { path: root.into() });
        }

        let root = root
            .canonicalize()
            .map_err(|source| NewError::RootDirCanonError {
                source,
                path: root.into(),
            })?;

        let goods_path = root.join(".goods");

        if !overwrite && goods_path.exists() {
            return Err(NewError::GoodsAlreadyExist {
                path: goods_path.into(),
            });
        }

        if overwrite && !goods_path.is_file() {
            return Err(NewError::GoodsAlreadyExist {
                path: goods_path.into(),
            });
        }

        let goods = Goods {
            inner: Inner {
                root: root.into(),
                assets_by_source_importer: HashMap::new(),
                assets_by_uuid: HashMap::new(),
                tags: HashSet::new(),
                importers: HashMap::new(),
                data: GoodsData {
                    assets: Vec::new(),
                    importer_dirs: HashSet::new(),
                },
            },
        };

        let file =
            std::fs::File::create(&goods_path).map_err(|source| SaveError::GoodsOpenError {
                source,
                path: goods_path.clone().into(),
            })?;

        bincode::serialize_into(file, &goods.inner.data).map_err(|source| {
            SaveError::BincodeError {
                source,
                path: goods_path.clone().into(),
            }
        })?;

        Ok(goods)
    }

    /// Opens goods storage from metadata file.
    #[tracing::instrument(skip(root), fields(root = %root.as_ref().display()))]
    pub fn open(root: impl AsRef<Path>) -> Result<Self, OpenError> {
        let root = root.as_ref();

        let root = root
            .canonicalize()
            .map_err(|source| OpenError::RootDirCanonError {
                source,
                path: root.into(),
            })?;

        let goods_path = root.join(".goods");

        let file =
            std::fs::File::open(&goods_path).map_err(|source| OpenError::GoodsOpenError {
                source,
                path: goods_path.clone().into(),
            })?;

        let mut data: GoodsData =
            bincode::deserialize_from(file).map_err(|source| OpenError::BincodeError {
                source,
                path: goods_path.clone().into(),
            })?;

        let mut tags = HashSet::<Arc<str>>::new();
        for asset in &mut data.assets {
            for tag in &mut *asset.tags {
                match tags.get(&**tag) {
                    Some(t) => *tag = t.clone(),
                    None => {
                        tags.insert(tag.clone());
                    }
                }
            }

            asset.source_absolute = root.join(&asset.source).into();
            asset.native_absolute = root.join(asset.uuid.to_hyphenated().to_string()).into();
        }

        let assets_by_uuid = data
            .assets
            .iter()
            .enumerate()
            .map(|(index, asset)| (asset.uuid, index))
            .collect();

        let assets_by_source_importer = data
            .assets
            .iter()
            .enumerate()
            .map(|(index, asset)| ((asset.source.clone(), asset.importer.clone()), index))
            .collect();

        let mut importers = HashMap::new();
        for dir_path in &data.importer_dirs {
            if let Err(err) = load_importers_dir(&mut importers, &root.join(dir_path)) {
                tracing::error!("Failed to scan importers dir: {:#}", err);
            }
        }

        Ok(Goods {
            inner: Inner {
                data,
                root: root.into(),
                tags,
                assets_by_uuid,
                assets_by_source_importer,
                importers,
            },
        })
    }

    pub fn save(&self) -> Result<(), SaveError> {
        let goods_path = self.inner.root.join(".goods");
        let file =
            std::fs::File::create(&goods_path).map_err(|source| SaveError::GoodsOpenError {
                source,
                path: goods_path.clone().into(),
            })?;
        bincode::serialize_into(file, &self.inner.data).map_err(|source| SaveError::BincodeError {
            source,
            path: goods_path.into(),
        })
    }

    #[tracing::instrument(skip(self, dir_path), fields(dir_path = %dir_path.as_ref().display()))]
    pub fn load_importers(&mut self, dir_path: impl AsRef<Path>) -> Result<(), ImporterDirError> {
        let dir_path = dir_path.as_ref();

        let dir_path_absolute = dir_path.canonicalize().map_err(|source| ImporterDirError {
            source,
            path: dir_path.into(),
        })?;

        let dir_path = relative_to(&dir_path_absolute, &self.inner.root);

        if !self.inner.data.importer_dirs.contains(dir_path.as_ref()) {
            load_importers_dir(&mut self.inner.importers, &dir_path_absolute).map_err(
                |source| ImporterDirError {
                    source,
                    path: dir_path.clone().into(),
                },
            )?;
            self.inner.data.importer_dirs.insert(dir_path.into());
        }
        Ok(())
    }

    /// Import asset into goods instance
    pub fn store(
        &mut self,
        source: impl AsRef<Path>,
        importer: &str,
        tags: &[&str],
    ) -> Result<Uuid, StoreError> {
        let source = source.as_ref();

        let source = if source.is_relative() {
            source
                .canonicalize()
                .map_err(|err| StoreError::SourceIoError {
                    path: source.into(),
                    source: err,
                })?
                .into()
        } else {
            source.into()
        };

        self.inner.store(source, importer, tags)
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

    fn fetch_impl(
        &mut self,
        uuid: &Uuid,
        next_version: u64,
    ) -> Result<Option<AssetData>, FetchError> {
        match self.inner.assets_by_uuid.get(uuid) {
            None => Err(FetchError::NotFound),
            Some(&index) => {
                let asset = &self.inner.data.assets[index];

                let mut native_file =
                    std::fs::File::open(&asset.native_absolute).map_err(|source| {
                        FetchError::NativeIoError {
                            source,
                            path: asset.native_absolute.to_path_buf().into(),
                        }
                    })?;

                let native_modified =
                    native_file
                        .metadata()
                        .and_then(|m| m.modified())
                        .map_err(|source| FetchError::NativeIoError {
                            source,
                            path: asset.native_absolute.to_path_buf().into(),
                        })?;

                if let Ok(source_modified) =
                    std::fs::metadata(&asset.source_absolute).and_then(|m| m.modified())
                {
                    if native_modified < source_modified {
                        tracing::trace!("Native asset file is out-of-date. Perform reimport");

                        match self.inner.importers.get(&*asset.importer) {
                            None => {
                                tracing::warn!(
                                    "Importer '{}' not found, asset '{}@{}' cannot be updated",
                                    asset.importer,
                                    asset.uuid,
                                    asset.source.display(),
                                );
                            }

                            Some(importer_entry) => {
                                let native_path_tmp = asset.native_absolute.with_extension("tmp");

                                let result = importer_entry.importer.clone().import(
                                    &asset.source_absolute.clone(),
                                    &native_path_tmp,
                                    &mut self.inner,
                                );

                                match result {
                                    Ok(()) => {
                                        let asset = &mut self.inner.data.assets[index];
                                        std::fs::rename(&native_path_tmp, &asset.native_absolute)
                                            .map_err(|source| FetchError::NativeIoError {
                                            source,
                                            path: asset.native_absolute.to_path_buf().into(),
                                        })?;
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
                    } else {
                        tracing::trace!("Native asset file is up-to-date");
                    }
                } else {
                    tracing::warn!("Failed to determine if native file is up-to-date");
                }

                if next_version > version_from_systime(native_modified) {
                    tracing::trace!("Native asset is not updated");
                    return Ok(None);
                }

                let asset = &self.inner.data.assets[index];
                let mut native_data = Vec::new();
                native_file
                    .read_to_end(&mut native_data)
                    .map_err(|source| FetchError::NativeIoError {
                        source,
                        path: asset.native_absolute.to_path_buf().into(),
                    })?;

                return Ok(Some(AssetData {
                    bytes: native_data.into_boxed_slice(),
                    version: version_from_systime(native_modified),
                }));
            }
        }
    }

    pub fn fetch_frozen(&self, uuid: &Uuid) -> Result<Option<AssetData>, FetchError> {
        match self.inner.assets_by_uuid.get(uuid) {
            None => Err(FetchError::NotFound),
            Some(&index) => {
                let asset = &self.inner.data.assets[index];

                let mut native_file =
                    std::fs::File::open(&asset.native_absolute).map_err(|source| {
                        FetchError::NativeIoError {
                            source,
                            path: asset.native_absolute.to_path_buf().into(),
                        }
                    })?;

                let native_modified = native_file
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .map_err(|source| FetchError::NativeIoError {
                        source,
                        path: asset.native_absolute.to_path_buf().into(),
                    })?;

                let mut native_data = Vec::new();
                native_file
                    .read_to_end(&mut native_data)
                    .map_err(|source| FetchError::NativeIoError {
                        source,
                        path: asset.native_absolute.to_path_buf().into(),
                    })?;

                return Ok(Some(AssetData {
                    bytes: native_data.into_boxed_slice(),
                    version: version_from_systime(native_modified),
                }));
            }
        }
    }
}

impl Inner {
    fn store(
        &mut self,
        source: Box<Path>,
        importer: &str,
        tags: &[&str],
    ) -> Result<Uuid, StoreError> {
        debug_assert!(source.as_ref().is_absolute());
        let source = Arc::<Path>::from(relative_to(&source, &self.root));

        let importer: Arc<str> = importer.into();

        if let Some(&index) = self
            .assets_by_source_importer
            .get(&(source.clone(), importer.clone()))
        {
            return Ok(self.data.assets[index].uuid);
        }

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

        let uuid = loop {
            let uuid = Uuid::new_v4();
            if !self.assets_by_uuid.contains_key(&uuid) {
                break uuid;
            }
        };

        match self.importers.get(&*importer) {
            None => Err(StoreError::ImporterNotFound {
                importer: importer.as_ref().to_owned(),
            }),
            Some(importer_entry) => {
                let source_absolute = self.root.join(&*source);
                let native_absolute = self.root.join(uuid.to_hyphenated().to_string());
                let native_path_tmp = native_absolute.with_extension("tmp");

                let result = importer_entry.importer.clone().import(
                    &source_absolute,
                    &native_path_tmp,
                    self,
                );

                if let Err(err) = result {
                    match err.downcast::<StoreError>() {
                        Ok(err) => return Err(*err),
                        Err(err) => return Err(StoreError::ImportError { source: err }),
                    }
                }

                if let Err(source) = std::fs::rename(native_path_tmp, &native_absolute) {
                    return Err(StoreError::NativeIoError {
                        path: native_absolute.into(),
                        source,
                    });
                }

                self.data.assets.push(Asset {
                    uuid,
                    importer: importer.clone(),
                    tags,
                    source_absolute: source_absolute.into(),
                    native_absolute: native_absolute.into(),
                    source: source.clone(),
                });

                self.assets_by_uuid.insert(uuid, self.data.assets.len() - 1);

                self.assets_by_source_importer
                    .insert((source, importer), self.data.assets.len() - 1);

                tracing::info!("Asset '{}' registered", uuid);
                Ok(uuid)
            }
        }
    }
}

impl goods_import::Registry for Inner {
    fn store(
        &mut self,
        source: &Path,
        importer: &str,
    ) -> Result<Uuid, Box<dyn Error + Send + Sync>> {
        match self.store(source.into(), importer, &[]) {
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
                match unsafe { lib.symbol::<fn() -> Vec<Arc<dyn Importer>>>("get_goods_importers") }
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

fn default_absolute_path_arc() -> Arc<Path> {
    PathBuf::new().into()
}

fn version_from_systime(systime: SystemTime) -> u64 {
    systime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn relative_to<'a>(path: &'a Path, root: &Path) -> std::borrow::Cow<'a, Path> {
    debug_assert!(path.is_absolute());
    debug_assert!(root.is_absolute());

    let mut pcs = path.components();
    let mut rcs = root.components();

    let prefix_length = pcs
        .by_ref()
        .zip(&mut rcs)
        .take_while(|(pc, rc)| pc == rc)
        .count();

    if prefix_length == 0 {
        path.into()
    } else {
        let mut pcs = path.components();
        pcs.nth(prefix_length - 1);

        let mut rcs = root.components();
        rcs.nth(prefix_length - 1);

        let up = (0..rcs.count()).fold(PathBuf::new(), |mut acc, _| {
            acc.push("..");
            acc
        });

        up.join(pcs.as_path()).into()
    }
}
