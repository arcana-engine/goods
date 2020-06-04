#[cfg(target_arch = "wasm32")]
core::compile_error!("This example cannot be built for wasm32 target");

use {
    goods::*,
    std::{collections::HashMap, path::Path},
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

/// We implement `SimpleAsset` for `Object` as it doesn't require any contextual conversion.
/// If asset type requires access to some context (like texture asset may require access to graphics context)
/// it must implemet `Asset` type then.
///
/// `SimpleAsset` implementations implement `Asset` automagically.
impl SimpleAsset for Object {}

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
    // Loading tasks will be spawned to `ThreadPool`.
    let cache = Cache::new(registry, futures_executor::ThreadPool::new().unwrap());

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

    while object_json.is_pending() || object_yaml.is_pending() {
        std::thread::yield_now();
    }

    log::info!("From json: {:#?}", object_json.get().unwrap());
    log::info!("From yaml: {:#?}", object_yaml.get().unwrap());
}
