use {
    goods::*, goods_fetch::FetchSource, goods_json::JsonFormat, goods_yaml::YamlFormat,
    std::collections::HashMap, wasm_bindgen::prelude::*,
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

#[wasm_bindgen]
pub async fn run() {
    // Init logging system.
    tracing_wasm::set_as_global_default();

    // Build source registry.
    // We'll use `String` as asset key.
    // Key type must be compatible with all used sources.
    let registry = LocalRegistry::builder()
        // One of the simplest sources is `FileSource`.
        // It reads asset data from files.
        // To get file path it joins root path with asset key.
        // asset key type must implement `AsRef<Path>`.
        // `String` type does.
        .with(FetchSource::new())
        .build();

    // Create new asset cache with built registry.
    let cache = LocalCache::new(registry);

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FetchSource` to fetch file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let object_json: LocalHandle<Object> = cache.load("asset.json".to_string());

    // Another asset will be "asset.yaml".
    // Again, file with the name will be fetched by `FetchSource` in the registry.
    // Alternative loading function accepts format for data decoding,
    // and here we specify `YamlFormat` to read YAML document from the file.
    let object_yaml: LocalHandle<Object> =
        cache.load_with_format("asset.yaml".to_string(), YamlFormat);

    // Await for handles treating them as `Future`.
    tracing::info!("From json: {:#?}", object_json.clone().await.unwrap_throw());
    tracing::info!("From yaml: {:#?}", object_yaml.clone().await.unwrap_throw());

    let document = web_sys::window().unwrap_throw().document().unwrap_throw();

    let div_json = document.create_element("div").unwrap_throw();
    div_json.set_inner_html(&format!("{:#?}", object_json.await.unwrap_throw()));
    document
        .body()
        .unwrap_throw()
        .append_child(&div_json)
        .unwrap_throw();

    let div_yaml = document.create_element("div").unwrap_throw();

    div_yaml.set_inner_html(&format!("{:#?}", object_yaml.await.unwrap_throw()));
    document
        .body()
        .unwrap_throw()
        .append_child(&div_yaml)
        .unwrap_throw();
}

fn main() {}
