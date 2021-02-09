use {
    cfg_if::cfg_if,
    goods::*,
    goods_fs::FileSource,
    goods_json::JsonFormat,
    goods_yaml::YamlFormat,
    std::{collections::HashMap, path::PathBuf},
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
impl AssetDefaultFormat for Object {
    type DefaultFormat = JsonFormat;
}

fn main() {
    // Init logging system.
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            wasm_bindgen_futures::spawn_local(run());
        } else {
            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(run());
        }
    }
}

async fn run() {
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
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        ))
        .build();

    // Create new asset cache with built registry.
    // Loading tasks will be spawned to `ThreadPool`.
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

    tracing::info!("From json: {:#?}", object_json.await.unwrap());
    tracing::info!("From yaml: {:#?}", object_yaml.await.unwrap());
}
