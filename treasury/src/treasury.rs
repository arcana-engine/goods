use {
    crate::{asset::Asset, import::Importers},
    std::{
        collections::{HashMap, HashSet},
        error::Error,
        io::Read,
        path::{Path, PathBuf},
        sync::Arc,
        time::SystemTime,
    },
    treasury_import::Importer,
    uuid::Uuid,
};

/// Storage for goods.
pub struct Treasury {
    inner: Inner,
}

struct Inner {
    /// All paths not suffixed with `_absolute` are relative to this.
    root: Box<Path>,

    // Data loaded from `root.join(".treasury")`.
    data: Data,

    /// Set of all tags.
    tags: HashSet<Arc<str>>,

    /// Lookup assets by uuid.
    assets_by_uuid: HashMap<Uuid, usize>,

    /// Lookup assets by source path and importer name combination.
    assets_by_source_importer: HashMap<(Arc<Path>, Arc<str>), usize>,

    /// Importers
    importers: Importers,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Data {
    /// Set of paths to directories from where importer libraries are loaded.
    importer_dirs: HashSet<Box<Path>>,

    /// Array with all registered assets.
    assets: Vec<Asset>,
}

pub struct AssetData {
    pub bytes: Box<[u8]>,
    pub version: u64,
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

#[derive(Debug, thiserror::Error)]
#[error("Failed to scan importers directory '{path}'")]
pub struct ImporterDirError {
    path: Box<Path>,
    source: std::io::Error,
}

impl Treasury {
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

        let goods = Treasury {
            inner: Inner {
                root: root.into(),
                assets_by_source_importer: HashMap::new(),
                assets_by_uuid: HashMap::new(),
                tags: HashSet::new(),
                importers: Importers::new(),
                data: Data {
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

        let mut data: Data =
            bincode::deserialize_from(file).map_err(|source| OpenError::BincodeError {
                source,
                path: goods_path.clone().into(),
            })?;

        let mut tags = HashSet::<Arc<str>>::new();
        for asset in &mut data.assets {
            asset.dedup_tags(&mut tags);
            asset.update_abs_paths(&root);
        }

        let assets_by_uuid = data
            .assets
            .iter()
            .enumerate()
            .map(|(index, asset)| (asset.uuid(), index))
            .collect();

        let assets_by_source_importer = data
            .assets
            .iter()
            .enumerate()
            .map(|(index, asset)| ((asset.source().clone(), asset.importer().clone()), index))
            .collect();

        Ok(Treasury {
            inner: Inner {
                importers: Importers::from_dirs(
                    data.importer_dirs.iter().map(|dir| root.join(dir)),
                ),
                data,
                root: root.into(),
                tags,
                assets_by_uuid,
                assets_by_source_importer,
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
            self.inner
                .importers
                .load_dir(&dir_path_absolute)
                .map_err(|source| ImporterDirError {
                    source,
                    path: dir_path.clone().into(),
                })?;
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
                let mut asset = &self.inner.data.assets[index];

                let mut native_file =
                    std::fs::File::open(asset.native_absolute()).map_err(|source| {
                        FetchError::NativeIoError {
                            source,
                            path: asset.native_absolute().to_path_buf().into(),
                        }
                    })?;

                let native_modified =
                    native_file
                        .metadata()
                        .and_then(|m| m.modified())
                        .map_err(|source| FetchError::NativeIoError {
                            source,
                            path: asset.native_absolute().to_path_buf().into(),
                        })?;

                if let Ok(source_modified) =
                    std::fs::metadata(asset.source_absolute()).and_then(|m| m.modified())
                {
                    if native_modified < source_modified {
                        tracing::trace!("Native asset file is out-of-date. Perform reimport");

                        match self.inner.importers.get_importer(asset.importer()) {
                            None => {
                                tracing::warn!(
                                    "Importer '{}' not found, asset '{}@{}' cannot be updated",
                                    asset.importer(),
                                    asset.uuid(),
                                    asset.source().display(),
                                );
                            }
                            Some(importer) => {
                                let native_path_tmp = asset.native_absolute().with_extension("tmp");

                                let result = importer.import(
                                    &asset.source_absolute().clone(),
                                    &native_path_tmp,
                                    &mut self.inner,
                                );
                                asset = &self.inner.data.assets[index];

                                match result {
                                    Ok(()) => {
                                        match std::fs::rename(
                                            &native_path_tmp,
                                            asset.native_absolute(),
                                        ) {
                                            Ok(()) => {
                                                tracing::trace!("Native file updated");
                                            }
                                            Err(err) => {
                                                tracing::warn!(
                                                            "Failed to copy native file '{}' from '{}'. {:#}",
                                                            asset.native_absolute().display(),
                                                            native_path_tmp.display(),
                                                            err
                                                        )
                                            }
                                        }
                                    }
                                    Err(err) => tracing::warn!(
                                        "Native file reimport failed '{:#}'. Fallback to old file",
                                        err
                                    ),
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

                let mut native_data = Vec::new();
                native_file
                    .read_to_end(&mut native_data)
                    .map_err(|source| FetchError::NativeIoError {
                        source,
                        path: asset.native_absolute().to_path_buf().into(),
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
            tracing::trace!("Already imported");
            return Ok(self.data.assets[index].uuid());
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

        match self.importers.get_importer(&*importer) {
            None => Err(StoreError::ImporterNotFound {
                importer: importer.as_ref().to_owned(),
            }),
            Some(importer_entry) => {
                tracing::trace!("Importer found");

                let source_absolute = self.root.join(&*source);
                let native_absolute = self.root.join(uuid.to_hyphenated().to_string());
                let native_path_tmp = native_absolute.with_extension("tmp");

                let result = importer_entry.import(&source_absolute, &native_path_tmp, self);

                if let Err(err) = result {
                    tracing::error!("Importer failed");
                    match err.downcast::<StoreError>() {
                        Ok(err) => return Err(*err),
                        Err(err) => return Err(StoreError::ImportError { source: err }),
                    }
                }

                tracing::trace!("Imported successfully");
                if let Err(source) = std::fs::rename(native_path_tmp, &native_absolute) {
                    return Err(StoreError::NativeIoError {
                        path: native_absolute.into(),
                        source,
                    });
                }

                self.data.assets.push(Asset::new(
                    uuid,
                    source.clone(),
                    importer.clone(),
                    tags,
                    source_absolute.into(),
                    native_absolute.into(),
                ));

                self.assets_by_uuid.insert(uuid, self.data.assets.len() - 1);

                self.assets_by_source_importer
                    .insert((source, importer), self.data.assets.len() - 1);

                tracing::info!("Asset '{}' registered", uuid);
                Ok(uuid)
            }
        }
    }
}

impl treasury_import::Registry for Inner {
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

fn version_from_systime(systime: SystemTime) -> u64 {
    systime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
