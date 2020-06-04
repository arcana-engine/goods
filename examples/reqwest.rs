#[cfg(target_arch = "wasm32")]
core::compile_error!("This example cannot be built for wasm32 target");

extern crate alloc;

use {goods::*, std::collections::HashMap};

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

// Let's make it tokio based app
#[tokio::main]
async fn main() {
    // Init loggger.
    env_logger::init();

    // Create new registry.
    let registry = Registry::builder()
        // With source which fetches assets by HTTP protocol
        // treating asset key as `URL`
        // asset key type is inferred as `&str` from `cache.load` below.
        .with(goods::ReqwestSource::new())
        .build();

    // Create new asset cache with built registry.
    // Spawn loading tasks in `tokio` runtime.
    let cache = Cache::new(registry, Tokio::current());

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let object_json: Handle<Object> =
        cache.load("https://raw.githubusercontent.com/zakarumych/goods/master/examples/asset.json");

    // Another asset will be "asset.yaml".
    // Again, sibling file with the name will be read by `FsSource` we added in the registry.
    // Alternative loading function accepts format for data decoding,
    // and here we specify `YamlFormat` to read YAML document from the file.
    let object_yaml: Handle<Object> = cache.load_with_format(
        "https://raw.githubusercontent.com/zakarumych/goods/master/examples/asset.yaml",
        YamlFormat,
    );

    // Await for handles treating them as `Future`.
    println!("From json: {:#?}", object_json.await);
    println!("From yaml: {:#?}", object_yaml.await);
}
