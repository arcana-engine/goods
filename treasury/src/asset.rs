use {
    std::{
        fmt::{self, Display},
        path::{Path, PathBuf},
    },
    uuid::Uuid,
};

/// Contains meta-information about an self.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Asset {
    /// Id of the self.
    uuid: Uuid,

    /// Path to source file.
    /// Relative to root path.
    source: Box<Path>,

    /// Source format of the asset.
    source_format: Box<str>,

    /// Native format of the asset.
    native_format: Box<str>,

    /// Arrays of tags associated with the self.
    tags: Box<[Box<str>]>,

    #[serde(skip, default = "default_absolute_path_box")]
    native_absolute: Box<Path>,

    /// Path to source file.
    #[serde(skip, default = "default_absolute_path_box")]
    source_absolute: Box<Path>,
}

impl Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(
                f,
                "{{\n\t{}\n\t {} as {}\n\t@ {} }}",
                self.uuid,
                self.source_format,
                self.native_format,
                self.source.display()
            )
        } else {
            write!(
                f,
                "{{ {} : '{}' as '{}' @ '{}' }}",
                self.uuid,
                self.source_format,
                self.native_format,
                self.source.display()
            )
        }
    }
}

impl Asset {
    pub fn new(
        uuid: Uuid,
        source: Box<Path>,
        source_format: Box<str>,
        native_format: Box<str>,
        tags: Box<[Box<str>]>,
        native_absolute: Box<Path>,
        source_absolute: Box<Path>,
    ) -> Asset {
        Asset {
            uuid,
            source,
            source_format,
            native_format,
            tags,
            native_absolute,
            source_absolute,
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn source_absolute(&self) -> &Path {
        &self.source_absolute
    }

    pub fn native_absolute(&self) -> &Path {
        &self.native_absolute
    }

    pub fn source_format(&self) -> &str {
        &self.source_format
    }

    pub fn native_format(&self) -> &str {
        &self.native_format
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn tags(&self) -> &[Box<str>] {
        &self.tags
    }

    pub fn update_abs_paths(&mut self, root: &Path) {
        self.source_absolute = root.join(&self.source).into();
        self.native_absolute = root
            .join(".treasury")
            .join(&self.uuid.to_hyphenated().to_string())
            .into();
    }
}

fn default_absolute_path_box() -> Box<Path> {
    PathBuf::new().into()
}
