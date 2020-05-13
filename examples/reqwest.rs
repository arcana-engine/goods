//!
//! Before running this example run `python -m http.server` in this example's directory.
//!

extern crate alloc;

use {
    goods::*,
    std::{collections::HashMap, sync::Arc, time::Duration},
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

    // Create new asset loader to drive async loading tasks.
    let mut loader = Loader::new();

    // Create new asset cache with built registry and loader.
    // Cache will issue loading tasks into this loader.
    // Note that `loader` is borrowed only for `Cache::new` function execution.
    let cache = Arc::new(Cache::new(registry, &loader));

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let object_json: Handle<Object> = cache.load("http://localhost:8000/asset.json");

    // Spawn a task that will await for all loads to complete.
    // `Loader::flush` will resolve one all pending loading tasks are complete.
    // `Loader::run` will resolve only when all caches created from it are dropped and all tasks are complete.
    tokio::spawn({
        let cache = cache.clone();
        async move {
            loader.run(&cache).await;
        }
    });

    // Another asset will be "asset.yaml".
    // Again, sibling file with the name will be read by `FsSource` we added in the registry.
    // Alternative loading function accepts format for data decoding,
    // and here we specify `YamlFormat` to read YAML document from the file.
    let object_yaml: Handle<Object> =
        cache.load_with_format("http://localhost:8000/asset.yaml", YamlFormat);

    // Spawn a task to complete assets loading.
    tokio::spawn(async move {
        loop {
            // Process all `Asset` implementations that need no real context to finish.
            // This is the case for `SimpleAsset` implementations.
            cache.process(&mut PhantomContext);

            // Make a 1ms delay between calls to the process function.
            tokio::time::delay_for(Duration::from_millis(1)).await;
        }
    });

    // Await for handles treating them as `Future`.
    println!("From json: {:#?}", object_json.await);
    println!("From yaml: {:#?}", object_yaml.await);
}
