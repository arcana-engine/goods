#[cfg(target_arch = "wasm32")]
core::compile_error!("This example cannot be built for wasm32 target");

use {
    futures_task::noop_waker_ref,
    goods::*,
    std::{collections::HashMap, path::Path, task::Context},
};

/// First we defined type to represent our assets.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
struct Object {
    key: Kind,

    #[serde(flatten)]
    rest: HashMap<String, String>,
}

/// `Kind` key from asset document will be parsed into this enum.
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq)]
enum Kind {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "yaml")]
    Yaml,
}

/// Possible errors include json and yaml errors.
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Json parsing error: {0}")]
    Json(serde_json::Error),

    #[error("Yaml parsing error: {0}")]
    Yaml(serde_yaml::Error),
}

/// To use out-of-the-box `JsonFormat` this error must be convertible from json error.
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

/// To use out-of-the-box `YamlFormat` this error must be convertible from json error.
impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::Yaml(err)
    }
}

/// We implement `SimpleAsset` for `Object` as it doesn't require any contextual conversion.
/// If asset type requires access to some context (like texture asset may require access to graphics context)
/// it must implemet `Asset` type then.
///
/// `SimpleAsset` implementations implement `Asset` automagically.
impl SimpleAsset for Object {
    type Error = Error;
}

/// Let's say that default format for `Object` is `JsonFormat`.
/// Only format types that implement `Default` may be default formats.
impl<K> AssetDefaultFormat<K> for Object {
    type DefaultFormat = JsonFormat;
}

fn main() {
    // Init logging system.
    env_logger::init();

    // Build source registry.
    // We'll use `String` as asset key.
    // Key type must be compatible with all used sources.
    let registry = Registry::<String>::builder()
        // One of the simplest sources is `FileSource`.
        // It reads asset data from files.
        // To get file path it joins root path with asset key.
        // asset key type must implement `AsRef<Path>`.
        // `String` type does.
        .with(FileSource::new(
            // Root dir will be parent dir of this file.
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples"),
        ))
        .build();

    // Create new asset cache with built registry.
    let cache = Cache::new(registry);

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let object_json: Handle<Object> = cache.load("asset.json".to_string());

    // Another asset will be "asset.yaml".
    // Again, sibling file with the name will be read by `FsSource` we added in the registry.
    // Alternative loading function accepts format for data decoding,
    // and here we specify `YamlFormat` to read YAML document from the file.
    let object_yaml: Handle<Object> = cache.load_with_format("asset.yaml".to_string(), YamlFormat);

    // Dummy async context.
    let mut ctx = Context::from_waker(noop_waker_ref());

    // Drive async loading tasks.
    // `FsSource` is not trully asynchronous as it uses `std::fs::File` API which is sync.
    // `FsSource::read` returns future that will be resolved on first `poll`.
    // So we expect loading to be finished after single call.
    let _ = cache.loader().poll(&mut ctx);

    // Process raw assets into their final form.
    // This variant of the function doesn't accept no context
    // and will affect only `SimpleAsset` implementations.
    //
    // more precisely, `Asset` implementations with `Context = PhantomContext`
    // which is the case for `SimpleAsset` implementations.
    cache.process_simple();

    log::info!("From json: {:#?}", object_json.get().unwrap());
    log::info!("From yaml: {:#?}", object_yaml.get().unwrap());
}
