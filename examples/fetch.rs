#[cfg(not(target_arch = "wasm32"))]
core::compile_error!("This example can be built only for wasm32 target");

use {
    goods::*,
    std::collections::HashMap,
    wasm_bindgen::{prelude::*, JsCast},
    wasm_bindgen_futures::spawn_local,
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

#[wasm_bindgen]
pub async fn run() {
    // Init logging system.
    console_log::init_with_level(log::Level::Trace).unwrap_throw();
    log::trace!("Running");

    // Build source registry.
    // We'll use `String` as asset key.
    // Key type must be compatible with all used sources.
    let registry = Registry::builder()
        // One of the simplest sources is `FileSource`.
        // It reads asset data from files.
        // To get file path it joins root path with asset key.
        // asset key type must implement `AsRef<Path>`.
        // `String` type does.
        .with(FetchSource::new())
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

    // Spawn a task that will await for all loads to complete.
    // This task will resolve only after `cache` is destroyed (all clones) which esnures that there will be no new tasks.
    spawn_local(cache.loader());

    // Process all `SimpleAsset` implementations.
    let closure = Closure::wrap(Box::new(move || cache.process_simple()) as Box<dyn Fn()>);
    let window = web_sys::window().unwrap_throw();
    window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            100,
        )
        .unwrap_throw();
    closure.forget();

    log::info!("Wait for assets");

    // Await for handles treating them as `Future`.
    log::info!("From json: {:#?}", object_json.await.unwrap_throw());
    log::info!("From yaml: {:#?}", object_yaml.await.unwrap_throw());
}
