use {
    std::{
        fmt::{self, Display},
        path::Path,
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
}

impl Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(
                f,
                "{{\n  uuid: {}\n  source: {}\n  source_format: {}\n  native_format: {}\n}}",
                self.uuid,
                self.source.display(),
                self.source_format,
                self.native_format,
            )
        } else {
            write!(
                f,
                "{{ {} <- '{}' : '{}' as '{}' }}",
                self.uuid,
                self.source.display(),
                self.source_format,
                self.native_format,
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
    ) -> Asset {
        Asset {
            uuid,
            source,
            source_format,
            native_format,
            tags,
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
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
}
