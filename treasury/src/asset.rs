use {
    std::{
        collections::HashSet,
        path::{Path, PathBuf},
        sync::Arc,
    },
    uuid::Uuid,
};

/// Contains meta-information about an self.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Asset {
    /// Id of the self.
    uuid: Uuid,

    /// Path to source file.
    /// Relative to root path.
    source: Arc<Path>,

    /// Importer for the self.
    importer: Arc<str>,

    /// Arrays of tags associated with the self.
    tags: Box<[Arc<str>]>,

    #[serde(skip, default = "default_absolute_path_arc")]
    native_absolute: Arc<Path>,

    /// Path to source file.
    #[serde(skip, default = "default_absolute_path_arc")]
    source_absolute: Arc<Path>,
}

impl Asset {
    pub fn new(
        uuid: Uuid,
        source: Arc<Path>,
        importer: Arc<str>,
        tags: Box<[Arc<str>]>,
        source_absolute: Arc<Path>,
        native_absolute: Arc<Path>,
    ) -> Asset {
        Asset {
            uuid,
            source,
            importer,
            tags,
            source_absolute,
            native_absolute,
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn source_absolute(&self) -> &Arc<Path> {
        &self.source_absolute
    }

    pub fn native_absolute(&self) -> &Arc<Path> {
        &self.native_absolute
    }

    pub fn importer(&self) -> &Arc<str> {
        &self.importer
    }

    pub fn source(&self) -> &Arc<Path> {
        &self.source
    }

    pub fn dedup_tags(&mut self, tags: &mut HashSet<Arc<str>>) {
        for tag in self.tags.iter_mut() {
            match tags.get(&**tag) {
                Some(t) => *tag = t.clone(),
                None => {
                    tags.insert(tag.clone());
                }
            }
        }
    }

    pub fn update_abs_paths(&mut self, root: &Path) {
        self.source_absolute = root.join(&self.source).into();
        self.native_absolute = root.join(&self.uuid.to_hyphenated().to_string()).into();
    }
}

fn default_absolute_path_arc() -> Arc<Path> {
    PathBuf::new().into()
}
